use super::*;
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::atomic::{AtomicBool, AtomicUsize, Ordering},
};

struct Server {
    url: String,
    mode: Arc<AtomicUsize>,
    stop: Arc<AtomicBool>,
}
impl Server {
    fn new(listener: TcpListener, initial: usize) -> Self {
        let url = format!("http://{}", listener.local_addr().unwrap());
        listener.set_nonblocking(true).unwrap();
        let mode = Arc::new(AtomicUsize::new(initial));
        let stop = Arc::new(AtomicBool::new(false));
        let (worker_mode, worker_stop) = (mode.clone(), stop.clone());
        std::thread::spawn(move || {
            while !worker_stop.load(Ordering::SeqCst) {
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
                    let health = String::from_utf8_lossy(&bytes).starts_with("GET /health ");
                    let (status, body) = match worker_mode.load(Ordering::SeqCst) {
                        // Closed without an answer, like a server resetting while it loads.
                        7 => continue,
                        1 => (503, "{}"),
                        5 => (500, "{}"),
                        6 if health => (
                            200,
                            r#"{"status":"loading","version":"0.5.6","device":"cpu"}"#,
                        ),
                        2 => (200, r#"{"status":"ok","server":"another app"}"#),
                        3 => (401, "{}"),
                        4 if !health => (200, r#"{"protocol":"other"}"#),
                        _ if health => (200, r#"{"status":"ok","version":"0.5.6","device":"cpu"}"#),
                        _ => (200, r#"{"protocol":"voicestudio.speech.v1"}"#),
                    };
                    let _ = write!(
                        stream,
                        "HTTP/1.1 {status} Test\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                        body.len()
                    );
                } else {
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        });
        Self { url, mode, stop }
    }
    fn bind(initial: usize) -> Self {
        Self::new(TcpListener::bind("127.0.0.1:0").unwrap(), initial)
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}
fn manager(launch: Launcher, budget: Duration) -> VoiceStartup {
    manager_with(launch, Arc::new(|_| None), budget)
}
fn manager_with(launch: Launcher, exited: Exited, budget: Duration) -> VoiceStartup {
    VoiceStartup::build(
        launch,
        exited,
        Arc::new(|_| {}),
        Arc::new(|_| {}),
        true,
        budget,
        Duration::from_millis(10),
    )
    .unwrap()
}
// A manager that records every warm-up request (`true`: a cold start).
fn warming(launch: Launcher) -> (VoiceStartup, Arc<Mutex<Vec<bool>>>) {
    let warms = Arc::new(Mutex::new(Vec::new()));
    let record = warms.clone();
    let service = VoiceStartup::build(
        launch,
        Arc::new(|_| None),
        Arc::new(|_| {}),
        Arc::new(move |cold| record.lock().unwrap().push(cold)),
        true,
        Duration::from_secs(12),
        Duration::from_millis(10),
    )
    .unwrap();
    (service, warms)
}
async fn warms_after(warms: &Arc<Mutex<Vec<bool>>>, count: usize) -> Vec<bool> {
    for _ in 0..100 {
        if warms.lock().unwrap().len() >= count {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
    warms.lock().unwrap().clone()
}
fn no_launch() -> Launcher {
    Arc::new(|_, _| panic!("must reuse the existing service"))
}

#[test]
fn automatic_launch_is_local_http_only() {
    assert_eq!(local_port("http://127.0.0.1:3900"), Some(3900));
    assert_eq!(local_port("http://localhost:4567"), Some(4567));
    for url in [
        "https://localhost:3900",
        "http://192.168.1.12:3900",
        "http://example.org",
        "http://localhost/proxy",
        "http://127.0.0.1:3900?key=x",
        "http://user@localhost",
    ] {
        assert_eq!(local_port(url), None, "{url}");
    }
}

#[test]
fn healthy_service_is_reused_and_rechecked() {
    tauri::async_runtime::block_on(async {
        let server = Server::bind(0);
        let service = manager(no_launch(), Duration::from_secs(2));
        service.ensure(&server.url).await.unwrap();
        assert_eq!(service.status().phase, "ready");
        server.mode.store(2, Ordering::SeqCst);
        assert!(
            service
                .ensure(&server.url)
                .await
                .unwrap_err()
                .contains("autre service")
        );
        server.mode.store(0, Ordering::SeqCst);
        service.ensure(&server.url).await.unwrap();
    });
}

#[test]
fn simultaneous_cold_requests_launch_once_and_wait_for_readiness() {
    tauri::async_runtime::block_on(async {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let server = Arc::new(Mutex::new(None));
        let launched = Arc::new(AtomicUsize::new(0));
        let (owner, count) = (server.clone(), launched.clone());
        let service = manager(
            Arc::new(move |port, _| {
                assert_eq!(port, address.port());
                count.fetch_add(1, Ordering::SeqCst);
                *owner.lock().unwrap() = Some(Server::new(TcpListener::bind(address).unwrap(), 0));
                Ok(())
            }),
            Duration::from_secs(12),
        );
        let url = format!("http://{address}");
        let (a, b, c) = tokio::join!(
            service.ensure(&url),
            service.ensure(&url),
            service.ensure(&url)
        );
        a.unwrap();
        b.unwrap();
        c.unwrap();
        assert_eq!(launched.load(Ordering::SeqCst), 1);
        service.ensure(&url).await.unwrap();
        assert_eq!(launched.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn existing_server_auth_and_protocol_errors_do_not_launch_an_app() {
    tauri::async_runtime::block_on(async {
        let server = Server::bind(3);
        let service = manager(no_launch(), Duration::from_secs(2));
        assert!(
            service
                .ensure(&server.url)
                .await
                .unwrap_err()
                .contains("401")
        );
        server.mode.store(4, Ordering::SeqCst);
        assert!(
            service
                .ensure(&server.url)
                .await
                .unwrap_err()
                .contains("protocole")
        );
    });
}

#[test]
fn startup_timeout_is_bounded_and_retry_can_succeed() {
    tauri::async_runtime::block_on(async {
        let server = Server::bind(1);
        let service = manager(no_launch(), Duration::from_millis(350));
        let error = service.ensure(&server.url).await.unwrap_err();
        assert!(error.contains("pas prêt"), "{error}");
        assert_eq!(service.status().phase, "error");
        server.mode.store(0, Ordering::SeqCst);
        service.ensure(&server.url).await.unwrap();
    });
}

#[test]
fn quit_cancels_waiters_and_prevents_later_launches() {
    tauri::async_runtime::block_on(async {
        let server = Server::bind(1);
        let service = manager(no_launch(), Duration::from_secs(60));
        let pending = service.ensure(&server.url);
        let cancel = async {
            tokio::time::sleep(Duration::from_millis(40)).await;
            service.cancel();
        };
        let (result, _) = tokio::join!(pending, cancel);
        assert!(result.unwrap_err().contains("annulé"));
        assert!(
            service
                .ensure(&server.url)
                .await
                .unwrap_err()
                .contains("annulé")
        );
    });
}

#[test]
fn changing_endpoint_cancels_old_startup_without_late_status() {
    tauri::async_runtime::block_on(async {
        let slow = Server::bind(1);
        let ready = Server::bind(0);
        let service = manager(no_launch(), Duration::from_millis(150));
        let a = service.ensure(&slow.url);
        let b = async {
            tokio::time::sleep(Duration::from_millis(30)).await;
            service.ensure(&ready.url).await
        };
        let (old, new) = tokio::join!(a, b);
        assert!(old.unwrap_err().contains("changé"));
        new.unwrap();
        tokio::time::sleep(Duration::from_millis(180)).await;
        assert_eq!(service.status().phase, "ready");
        assert_eq!(service.status().url, ready.url);
    });
}

#[test]
fn launch_failure_is_reported_and_can_be_retried() {
    tauri::async_runtime::block_on(async {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", reservation.local_addr().unwrap());
        drop(reservation);
        let count = Arc::new(AtomicUsize::new(0));
        let launches = count.clone();
        let service = manager(
            Arc::new(move |_, _| {
                launches.fetch_add(1, Ordering::SeqCst);
                Err("VoiceStudio introuvable.".into())
            }),
            Duration::from_secs(12),
        );
        for _ in 0..2 {
            assert!(
                service
                    .ensure(&url)
                    .await
                    .unwrap_err()
                    .contains("introuvable")
            );
        }
        assert_eq!(count.load(Ordering::SeqCst), 2);
    });
}

#[test]
fn a_loading_server_stays_starting_until_it_answers() {
    tauri::async_runtime::block_on(async {
        // 500, "loading" health and a reset connection are not failures.
        for mode in [1, 5, 6, 7] {
            let server = Server::bind(mode);
            let service = manager(no_launch(), Duration::from_secs(10));
            let heal = async {
                tokio::time::sleep(Duration::from_millis(300)).await;
                let status = service.status();
                assert_eq!(status.phase, "starting", "mode {mode}");
                assert!(status.message.contains("charge ses modèles"), "{status:?}");
                server.mode.store(0, Ordering::SeqCst);
            };
            let (result, _) = tokio::join!(service.ensure(&server.url), heal);
            result.unwrap();
            assert_eq!(service.status().phase, "ready", "mode {mode}");
        }
    });
}

#[test]
fn a_slow_cold_start_is_waited_for_while_the_process_lives() {
    tauri::async_runtime::block_on(async {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let launched = Arc::new(AtomicUsize::new(0));
        let count = launched.clone();
        // The process is alive but listens only after loading its models.
        let service = manager(
            Arc::new(move |_, _| {
                count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }),
            Duration::from_secs(20),
        );
        let url = format!("http://{address}");
        let watch = async {
            let loaded = async {
                while !service.status().message.contains("charge ses modèles") {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
            };
            tokio::time::timeout(Duration::from_secs(10), loaded)
                .await
                .expect("the loading message must appear");
            assert_eq!(service.status().phase, "starting");
            Server::new(TcpListener::bind(address).unwrap(), 0)
        };
        let (result, _server) = tokio::join!(service.ensure(&url), watch);
        result.unwrap();
        assert_eq!(service.status().phase, "ready");
        assert_eq!(launched.load(Ordering::SeqCst), 1);
    });
}

#[test]
fn a_dead_process_is_reported_without_waiting_for_the_budget() {
    tauri::async_runtime::block_on(async {
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", reservation.local_addr().unwrap());
        drop(reservation);
        let service = manager_with(
            Arc::new(|_, _| Ok(())),
            Arc::new(|_| Some("1".into())),
            Duration::from_secs(60),
        );
        let started = Instant::now();
        let error = service.ensure(&url).await.unwrap_err();
        assert!(
            error.contains("arrêté") && error.contains("code 1"),
            "{error}"
        );
        assert_eq!(service.status().phase, "error");
        assert!(started.elapsed() < Duration::from_secs(5));
    });
}

#[test]
fn a_definitive_http_answer_is_still_an_error() {
    tauri::async_runtime::block_on(async {
        let server = Server::bind(3);
        let service = manager(no_launch(), Duration::from_secs(30));
        let started = Instant::now();
        assert!(service.ensure(&server.url).await.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    });
}

#[test]
fn the_voice_catalog_is_retried_only_for_what_a_loading_server_does() {
    let at = |error: DiscoveryError, attempt, secs| {
        catalog_retry(&error, attempt, Duration::from_secs(secs))
    };
    let http = |code| DiscoveryError::Http(code, "/models".into());
    // Unreachable, timed out, 5xx: wait, with a growing but capped backoff.
    assert_eq!(
        at(DiscoveryError::Unreachable, 0, 0),
        Some(Duration::from_millis(250))
    );
    assert_eq!(
        at(DiscoveryError::Timeout, 2, 3),
        Some(Duration::from_secs(1))
    );
    assert_eq!(at(http(503), 9, 3), Some(Duration::from_secs(2)));
    // The patience is bounded, and the last wait never overshoots it.
    assert_eq!(at(http(500), 9, 19), Some(Duration::from_secs(1)));
    assert_eq!(at(http(500), 9, 20), None);
    // Definitive answers are reported at once.
    for error in [
        http(404),
        http(422),
        DiscoveryError::Authentication,
        DiscoveryError::TooLarge,
        DiscoveryError::Format("/models".into()),
        DiscoveryError::Address("x".into()),
    ] {
        assert_eq!(at(error, 0, 0), None);
    }
}

#[test]
fn warm_up_is_requested_when_ready_and_cold_only_after_a_start() {
    tauri::async_runtime::block_on(async {
        // Already running: ready at the first probe, never cold.
        let server = Server::bind(0);
        let (service, warms) = warming(no_launch());
        service.ensure(&server.url).await.unwrap();
        assert_eq!(warms_after(&warms, 1).await, [false]);
        // Not an error path: a definitive refusal never warms anything.
        server.mode.store(3, Ordering::SeqCst);
        assert!(service.ensure(&server.url).await.is_err());
        assert_eq!(warms_after(&warms, 2).await, [false]);

        // Started by Sherka: cold.
        let reservation = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = reservation.local_addr().unwrap();
        drop(reservation);
        let started = Arc::new(Mutex::new(None));
        let owner = started.clone();
        let (service, warms) = warming(Arc::new(move |_, _| {
            *owner.lock().unwrap() = Some(Server::new(TcpListener::bind(address).unwrap(), 0));
            Ok(())
        }));
        service.ensure(&format!("http://{address}")).await.unwrap();
        assert_eq!(warms_after(&warms, 1).await, [true]);
    });
}
