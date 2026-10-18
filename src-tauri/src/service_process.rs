//! Launch the installed VoiceStudio desktop app, never commands from a profile.
#[cfg(windows)]
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};
use tokio_util::sync::CancellationToken;

#[cfg(windows)]
fn running(name: &str) -> Result<bool, String> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
        System::Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
            TH32CS_SNAPPROCESS,
        },
    };
    // Snapshot is owned here and closed on every path. No process is opened or killed.
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0);
        if snapshot == INVALID_HANDLE_VALUE {
            return Err("Impossible de vérifier les applications déjà ouvertes.".into());
        }
        let mut entry: PROCESSENTRY32W = std::mem::zeroed();
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut found = false;
        let mut valid = Process32FirstW(snapshot, &mut entry);
        while valid != 0 {
            let length = entry
                .szExeFile
                .iter()
                .position(|x| *x == 0)
                .unwrap_or(entry.szExeFile.len());
            if String::from_utf16_lossy(&entry.szExeFile[..length]).eq_ignore_ascii_case(name) {
                found = true;
                break;
            }
            valid = Process32NextW(snapshot, &mut entry);
        }
        CloseHandle(snapshot);
        Ok(found)
    }
}

#[cfg(windows)]
fn registered_executable(name: &str, file: &str) -> Option<PathBuf> {
    use winreg::{
        RegKey,
        enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
    };
    for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
        for subkey in [
            r"Software\Microsoft\Windows\CurrentVersion\Uninstall",
            r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
        ] {
            let Ok(root) = RegKey::predef(hive).open_subkey(subkey) else {
                continue;
            };
            for key in root.enum_keys().flatten() {
                let Ok(entry) = root.open_subkey(key) else {
                    continue;
                };
                let label: String = entry.get_value("DisplayName").unwrap_or_default();
                if label != name && !label.starts_with(&format!("{name} ")) {
                    continue;
                }
                if let Ok(directory) = entry.get_value::<String, _>("InstallLocation") {
                    let path = PathBuf::from(directory).join(file);
                    if path.is_file() {
                        return Some(path);
                    }
                }
                if let Ok(icon) = entry.get_value::<String, _>("DisplayIcon") {
                    let path = PathBuf::from(icon.trim().trim_end_matches(",0").trim_matches('"'));
                    if path
                        .file_name()
                        .is_some_and(|n| n.to_string_lossy().eq_ignore_ascii_case(file))
                        && path.is_file()
                    {
                        return Some(path);
                    }
                }
            }
        }
    }
    None
}

#[cfg(windows)]
fn app_executable() -> Option<PathBuf> {
    let name = "VoiceStudio";
    let file = "VoiceStudio.exe";
    let mut candidates = Vec::new();
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        candidates.push(PathBuf::from(local).join("Programs").join(name).join(file));
    }
    for variable in ["ProgramFiles", "ProgramFiles(x86)"] {
        if let Some(root) = std::env::var_os(variable) {
            candidates.push(PathBuf::from(root).join(name).join(file));
        }
    }
    candidates.into_iter().find(|p| p.is_file()).or_else(|| {
        #[cfg(windows)]
        {
            registered_executable(name, file)
        }
        #[cfg(not(windows))]
        {
            None
        }
    })
}

#[cfg(windows)]
fn command(path: &Path) -> Command {
    let mut command = Command::new(path);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if let Some(directory) = path.parent() {
        command.current_dir(directory);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW: no console flash.
    }
    command
}

#[cfg(windows)]
fn minimize_launched_voice(pid: u32, cancel: CancellationToken) {
    // VoiceStudio 0.5.6 has no background CLI flag. Minimize only the window
    // of the process we just created, once, leaving existing user windows alone.
    std::thread::spawn(move || {
        use windows_sys::Win32::{
            Foundation::{HWND, LPARAM},
            UI::WindowsAndMessaging::{
                EnumWindows, GetWindowThreadProcessId, IsWindowVisible, SW_MINIMIZE,
                ShowWindowAsync,
            },
        };
        use windows_sys::core::BOOL;
        struct Search {
            pid: u32,
            found: bool,
        }
        unsafe extern "system" fn visit(window: HWND, data: LPARAM) -> BOOL {
            unsafe {
                let search = &mut *(data as *mut Search);
                let mut owner = 0;
                GetWindowThreadProcessId(window, &mut owner);
                if owner == search.pid && IsWindowVisible(window) != 0 {
                    ShowWindowAsync(window, SW_MINIMIZE);
                    search.found = true;
                    return 0;
                }
            }
            1
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        while Instant::now() < deadline && !cancel.is_cancelled() {
            let mut search = Search { pid, found: false };
            unsafe {
                EnumWindows(Some(visit), &mut search as *mut _ as LPARAM);
            }
            if search.found {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    });
}

pub fn launch(port: u16, cancel: CancellationToken) -> Result<(), String> {
    #[cfg(not(windows))]
    {
        let _ = (port, cancel);
        Err("Le lancement automatique de VoiceStudio est disponible sur Windows.".into())
    }
    #[cfg(windows)]
    {
        if cancel.is_cancelled() {
            return Err("Démarrage annulé.".into());
        }
        if running("VoiceStudio.exe")? {
            return Ok(());
        }
        let executable = app_executable().ok_or(
            "VoiceStudio est introuvable sur ce PC. Installe-le ou ouvre-le, puis réessaie.",
        )?;
        let mut child = command(&executable);
        child.env("OMNIVOICE_PORT", port.to_string());
        if cancel.is_cancelled() {
            return Err("Démarrage annulé.".into());
        }
        let process = child
            .spawn()
            .map_err(|e| format!("Impossible de démarrer VoiceStudio : {e}"))?;
        minimize_launched_voice(process.id(), cancel);
        // VoiceStudio deliberately survives Sherka and stays available to other clients.
        drop(process);
        Ok(())
    }
}
