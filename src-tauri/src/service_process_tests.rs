use super::*;
#[test]
fn explicit_runtime_paths_with_spaces_are_resolved_without_launching_a_desktop_app() {
    let root = tempfile::tempdir().unwrap();
    let project = root.path().join("Voice Studio custom");
    std::fs::create_dir_all(project.join("backend")).unwrap();
    std::fs::write(project.join("backend/main.py"), "").unwrap();
    let python = venv(&project);
    std::fs::create_dir_all(python.parent().unwrap()).unwrap();
    std::fs::write(&python, "").unwrap();
    let config = VoiceServiceConfig {
        directory: project.display().to_string(),
        ..Default::default()
    };
    assert_eq!(resolve(&config).unwrap(), (project.clone(), python));
    let custom = root.path().join("custom python");
    std::fs::write(&custom, "").unwrap();
    assert_eq!(
        resolve(&VoiceServiceConfig {
            python: custom.display().to_string(),
            ..config
        })
        .unwrap(),
        (project, custom)
    );
    assert!(
        resolve(&VoiceServiceConfig {
            directory: root.path().join("missing").display().to_string(),
            ..Default::default()
        })
        .is_err()
    );
}
fn alive(pid: u32) -> bool {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, STILL_ACTIVE},
            System::Threading::{
                GetExitCodeProcess, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
            },
        };
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let mut code = 0;
        let running = GetExitCodeProcess(handle, &mut code) != 0 && code == STILL_ACTIVE as u32;
        CloseHandle(handle);
        running
    }
    #[cfg(not(windows))]
    {
        // A killed orphan may linger as a zombie until its new parent reaps it.
        std::fs::read_to_string(format!("/proc/{pid}/stat"))
            .is_ok_and(|stat| !stat.rsplit(") ").next().unwrap_or("").starts_with('Z'))
    }
}
fn wait_until(mut what: impl FnMut() -> bool) -> bool {
    for _ in 0..200 {
        if what() {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(100));
    }
    false
}
/// A server that spawns a worker, like a Python server with child processes.
/// Returns the owner, the worker's pid and the directory holding the pid file.
fn server_with_worker() -> (Process, u32, tempfile::TempDir) {
    let directory = tempfile::tempdir().unwrap();
    let file = directory.path().join("worker.pid");
    let mut command = if cfg!(windows) {
        let mut command = Command::new("powershell.exe");
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "$w = Start-Process powershell.exe -ArgumentList '-NoProfile','-Command','Start-Sleep -Seconds 120' -WindowStyle Hidden -PassThru; Set-Content -NoNewline -Path $env:SHERKA_TEST_PIDFILE -Value $w.Id; Start-Sleep -Seconds 120",
        ]);
        command
    } else {
        let mut command = Command::new("sh");
        command.args(["-c", "sleep 120 & echo $! > \"$SHERKA_TEST_PIDFILE\"; wait"]);
        command
    };
    command.env("SHERKA_TEST_PIDFILE", &file);
    let owner = Process::spawn(&mut command).unwrap();
    assert!(wait_until(
        || std::fs::read_to_string(&file).is_ok_and(|t| !t.trim().is_empty())
    ));
    let worker = std::fs::read_to_string(&file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(alive(worker));
    (owner, worker, directory)
}
#[test]
fn stopping_the_owner_closes_the_server_and_its_workers() {
    let (mut owner, worker, _directory) = server_with_worker();
    assert!(owner.stop());
    assert!(owner.try_wait().unwrap().is_some());
    assert!(
        wait_until(|| !alive(worker)),
        "the worker outlived its server"
    );
    // A second shutdown has nothing left to close.
    assert!(owner.stop());
}
#[test]
fn dropping_the_owner_closes_the_workers_like_a_crash_would() {
    let (owner, worker, _directory) = server_with_worker();
    drop(owner);
    assert!(
        wait_until(|| !alive(worker)),
        "the worker outlived its owner"
    );
}
#[test]
fn a_dead_server_leaves_no_worker_holding_its_port() {
    let (mut owner, worker, _directory) = server_with_worker();
    // The server dies on its own while the worker lives.
    owner.kill_leader_only();
    assert!(wait_until(|| owner.try_wait().unwrap().is_some()));
    // The death is noticed (as the startup wait does): the rest of the tree goes too.
    assert!(
        wait_until(|| !alive(worker)),
        "the worker outlived its server"
    );
}
#[test]
fn a_server_that_was_already_running_is_never_owned_nor_closed() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    // No runtime to start: the launch fails without registering any process.
    let config = VoiceServiceConfig {
        autostart: true,
        directory: tempfile::tempdir()
            .unwrap()
            .path()
            .join("none")
            .display()
            .to_string(),
        ..Default::default()
    };
    let logs = tempfile::tempdir().unwrap();
    assert!(launch(port, CancellationToken::new(), config, logs.path().into()).is_err());
    assert!(
        !CHILDREN
            .lock()
            .unwrap()
            .contains_key(&format!("127.0.0.1:{port}"))
    );
    assert_eq!(shutdown(), Closed::default());
    assert_eq!(exited(port), None);
    assert!(std::net::TcpStream::connect(("127.0.0.1", port)).is_ok());
}
#[test]
fn the_exit_line_says_what_became_of_voicestudio() {
    let closed = |stopped, failed| Closed { stopped, failed };
    assert_eq!(
        exit_event(Some(&closed(1, 0)), true),
        Some("voicestudio_stopped")
    );
    assert_eq!(
        exit_event(Some(&closed(1, 1)), true),
        Some("voicestudio_stop_failed")
    );
    assert_eq!(exit_event(None, false), Some("voicestudio_stop_failed"));
    // Up but not ours: left alone. Down and not ours: nothing to say.
    assert_eq!(
        exit_event(Some(&closed(0, 0)), true),
        Some("voicestudio_left_running")
    );
    assert_eq!(exit_event(Some(&closed(0, 0)), false), None);
}
