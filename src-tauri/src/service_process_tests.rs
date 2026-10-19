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
#[test]
fn owned_process_can_stop_only_its_own_child() {
    let mut command = Command::new(if cfg!(windows) {
        "powershell.exe"
    } else {
        "sleep"
    });
    if cfg!(windows) {
        command.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Start-Sleep -Seconds 60",
        ]);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
    } else {
        command.arg("60");
    }
    let mut child = Process::spawn(&mut command).unwrap();
    assert!(child.try_wait().unwrap().is_none());
    child.stop();
    assert!(child.try_wait().unwrap().is_some());
}
