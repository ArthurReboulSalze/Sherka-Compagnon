use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
}
impl Server {
    fn new(listener: TcpListener, mode: u16) -> Self {
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        std::thread::spawn(move || {
            while !done.load(Ordering::SeqCst) {
                if let Ok((mut stream, _)) = listener.accept() {
                    stream.set_nonblocking(false).unwrap();
                    stream
                        .set_read_timeout(Some(Duration::from_secs(1)))
                        .unwrap();
                    let mut bytes = Vec::new();
                    while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 4096 {
                        let mut byte = [0];
                        if stream.read_exact(&mut byte).is_err() {
                            break;
                        }
                        bytes.push(byte[0]);
                    }
                    let body = if mode == 200 {
                        r#"{"system":{"comfyui_version":"0.39.0"},"devices":[]}"#
                    } else {
                        "{}"
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {mode} Test\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Self { url, stop }
    }
    fn bind(mode: u16) -> Self {
        Self::new(TcpListener::bind("127.0.0.1:0").unwrap(), mode)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
fn config() -> ComfyStartupConfig {
    ComfyStartupConfig {
        disabled: false,
        directory: "explicitly-selected-installation".into(),
        python: String::new(),
    }
}
fn manager(launcher: Launcher, budget: Duration) -> ComfyStartup {
    ComfyStartup::build(launcher, Arc::new(|_| {}), budget).unwrap()
}
fn no_launch() -> Launcher {
    Arc::new(|_, _, _| panic!("must not launch a program"))
}

#[test]
fn auto_start_is_loopback_http_only_and_requires_an_explicit_device_path() {
    for value in [
        "http://localhost:8188",
        "http://127.0.0.1:8188",
        "http://127.0.0.2:8188",
        "http://[::1]:8188",
    ] {
        assert!(local_url(value).is_some());
    }
    for value in [
        "http://192.168.1.77:8188",
        "http://comfy.example:8188",
        "https://localhost:8188",
        "http://localhost:8188/api",
        "http://user:password@localhost:8188",
        "http://localhost:8188?x=1",
    ] {
        assert!(local_url(value).is_none());
        let service = manager(no_launch(), Duration::from_secs(1));
        assert_eq!(service.start(value, &config(), false).unwrap(), 0);
    }
    let service = manager(no_launch(), Duration::from_secs(1));
    assert_eq!(
        service
            .start("http://127.0.0.1:8188", &Default::default(), false)
            .unwrap(),
        0
    );
    tauri::async_runtime::block_on(async {
        let token = CancellationToken::new();
        token.cancel();
        assert!(
            service
                .ensure("http://127.0.0.1:8188", &config(), &token, false)
                .await
                .is_err()
        );
    });
}

#[test]
fn existing_servers_are_reused_and_http_errors_never_launch_another_process() {
    tauri::async_runtime::block_on(async {
        for mode in [200, 401, 503] {
            let server = Server::bind(mode);
            let service = manager(no_launch(), Duration::from_secs(10));
            let result = service
                .ensure(&server.url, &config(), &CancellationToken::new(), true)
                .await;
            assert_eq!(result.is_ok(), mode == 200, "{result:?}");
            if mode == 200 {
                service
                    .ensure(&server.url, &config(), &CancellationToken::new(), true)
                    .await
                    .unwrap();
            }
        }
    });
}

#[test]
fn concurrent_waiters_share_one_launch_and_wait_for_actual_readiness() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let count = Arc::new(AtomicUsize::new(0));
    let server = Arc::new(Mutex::new(None::<Server>));
    let (calls, owner) = (count.clone(), server.clone());
    let service = manager(
        Arc::new(move |_, url, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            *owner.lock().unwrap() = Some(Server::new(
                TcpListener::bind(("127.0.0.1", url.port().unwrap())).unwrap(),
                200,
            ));
            Ok(None)
        }),
        Duration::from_secs(4),
    );
    tauri::async_runtime::block_on(async {
        let config = config();
        let token = CancellationToken::new();
        let (a, b) = tokio::join!(
            service.ensure(&url, &config, &token, false),
            service.ensure(&url, &config, &token, false)
        );
        a.unwrap();
        b.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert_eq!(service.status().phase, "ready");
        service.ensure(&url, &config, &token, true).await.unwrap();
        assert_eq!(count.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn timeout_is_bounded_does_not_retry_by_itself_and_shutdown_cancels_waiters() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    let count = Arc::new(AtomicUsize::new(0));
    let calls = count.clone();
    let service = manager(
        Arc::new(move |_, _, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }),
        Duration::from_secs(4),
    );
    tauri::async_runtime::block_on(async {
        assert!(
            service
                .ensure(&url, &config(), &CancellationToken::new(), false)
                .await
                .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        assert!(
            service
                .ensure(&url, &config(), &CancellationToken::new(), false)
                .await
                .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
        service.cancel();
        assert!(
            service
                .ensure(&url, &config(), &CancellationToken::new(), true)
                .await
                .is_err()
        );
        assert_eq!(count.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn python_command_is_direct_quoted_by_os_loopback_bound_and_headless() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("Comfy UI unicode");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("main.py"), "").unwrap();
    let python = dir.path().join("python test executable");
    std::fs::write(&python, "").unwrap();
    let config = ComfyStartupConfig {
        disabled: false,
        directory: folder.display().to_string(),
        python: python.display().to_string(),
    };
    let command = launch_command(
        &config,
        &url::Url::parse("http://127.0.0.1:8199").unwrap(),
        dir.path(),
    )
    .unwrap();
    let args = command
        .get_args()
        .map(|a| a.to_string_lossy().to_string())
        .collect::<Vec<_>>();
    assert_eq!(
        args,
        [
            "-s",
            "main.py",
            "--listen",
            "127.0.0.1",
            "--port",
            "8199",
            "--disable-auto-launch"
        ]
    );
    assert_eq!(command.get_program(), python);
    assert_eq!(
        command.get_current_dir(),
        Some(folder.canonicalize().unwrap().as_path())
    );
    let token = CancellationToken::new();
    token.cancel();
    assert!(
        launch(
            &config,
            &url::Url::parse("http://127.0.0.1:8199").unwrap(),
            dir.path(),
            &token
        )
        .is_err()
    );
}

#[test]
#[cfg(windows)]
fn portable_python_is_found_from_the_root_or_inner_comfy_folder() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("ComfyUI");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("main.py"), "").unwrap();
    let embedded = dir.path().join("python_embeded");
    std::fs::create_dir(&embedded).unwrap();
    let python = embedded.join("python.exe");
    std::fs::write(&python, "").unwrap();
    for selected in [dir.path(), folder.as_path()] {
        let config = ComfyStartupConfig {
            disabled: false,
            directory: selected.display().to_string(),
            python: String::new(),
        };
        let command = launch_command(
            &config,
            &url::Url::parse("http://127.0.0.1:8188").unwrap(),
            dir.path(),
        )
        .unwrap();
        assert_eq!(
            std::fs::canonicalize(command.get_program()).unwrap(),
            python.canonicalize().unwrap()
        );
    }
}
