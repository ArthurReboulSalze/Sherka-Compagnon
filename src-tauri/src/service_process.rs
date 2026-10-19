//! Start the existing VoiceStudio Python runtime without an Electron window.
use crate::owned_process::Process;
use sherka_core::model::VoiceServiceConfig;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{LazyLock, Mutex},
};
use tokio_util::sync::CancellationToken;
static CHILDREN: LazyLock<Mutex<HashMap<String, Process>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
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

fn runtime_home() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("VoiceStudio"))
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|p| PathBuf::from(p).join(".config")))
            .map(|p| p.join("VoiceStudio"))
    }
}
fn read_json(path: &Path) -> Option<serde_json::Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}
fn venv(root: &Path) -> PathBuf {
    root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    })
}
pub fn resolve(config: &VoiceServiceConfig) -> Result<(PathBuf, PathBuf), String> {
    let mut candidates = Vec::new();
    if !config.directory.is_empty() {
        let path = PathBuf::from(&config.directory);
        let root = if path.is_file() {
            path.parent().unwrap_or(&path).to_owned()
        } else {
            path
        };
        candidates.extend([
            root.clone(),
            root.join("project"),
            root.join("runtime/project"),
        ]);
        if !config.python.is_empty() {
            candidates.push(root.join("resources"));
        }
        // A desktop installation stores its prepared Python runtime in the
        // user's data directory; a source checkout can supply its own .venv.
        if !root.join("resources/backend/main.py").is_file()
            && !root.join("backend/main.py").is_file()
            && !root.join("project/backend/main.py").is_file()
            && !root.join("runtime/project/backend/main.py").is_file()
        {
            return Err("Chemin VoiceStudio invalide. Choisis son installation ou le dossier du serveur Python.".into());
        }
    }
    if let Some(home) = runtime_home() {
        if let Some(value) = read_json(&home.join("runtime-location.json"))
            && let Some(root) = value["root"]
                .as_str()
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        {
            candidates.push(root.join("project"));
        }
        candidates.push(home.join("runtime/project"));
    }
    #[cfg(windows)]
    if config.directory.is_empty()
        && !config.python.is_empty()
        && let Some(exe) = app_executable()
    {
        candidates.push(exe.parent().unwrap().join("resources"));
    }
    for root in candidates {
        let python = if config.python.is_empty() {
            venv(&root)
        } else {
            PathBuf::from(&config.python)
        };
        if root.join("backend/main.py").is_file() && python.is_file() {
            return Ok((root, python));
        }
    }
    Err("Serveur VoiceStudio introuvable. Ouvre VoiceStudio une première fois pour préparer son environnement, ou indique son dossier et son Python dans Réglages.".into())
}
fn storage_environment(command: &mut Command) {
    #[cfg(windows)]
    for base in ["LOCALAPPDATA", "APPDATA"] {
        let Some(root) = std::env::var_os(base) else {
            continue;
        };
        let Some(config) =
            read_json(&PathBuf::from(root).join("com.debpalash.omnivoice-studio/config.json"))
        else {
            continue;
        };
        let text = |a: &str, b: &str| {
            config[a]
                .as_str()
                .or(config[b].as_str())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
        };
        let portable = config["install_mode"]
            .as_str()
            .or(config["installMode"].as_str())
            == Some("portable");
        let (data, models) = if portable {
            let root = text("portable_dir", "portableDir");
            (
                root.as_ref().map(|p| p.join("data")),
                root.map(|p| p.join("data/models")),
            )
        } else {
            (text("data_dir", "dataDir"), text("models_dir", "modelsDir"))
        };
        let found = data.is_some() || models.is_some();
        for (key, value) in [
            ("OMNIVOICE_DATA_DIR", data),
            ("OMNIVOICE_CACHE_DIR", models),
        ] {
            if std::env::var_os(key).is_none()
                && let Some(value) = value
            {
                command.env(key, value);
            }
        }
        if found {
            break;
        }
    }
    let _ = command;
}
pub fn launch(
    port: u16,
    cancel: CancellationToken,
    config: VoiceServiceConfig,
    logs: PathBuf,
) -> Result<(), String> {
    if !config.autostart {
        return Err(
            "VoiceStudio est arrêté. Démarre-le ou active son démarrage automatique dans Réglages."
                .into(),
        );
    }
    let endpoint = url::Url::parse(&config.url).map_err(|_| "Adresse VoiceStudio invalide.")?;
    let host = if endpoint.host_str() == Some("[::1]") || endpoint.host_str() == Some("::1") {
        "::1"
    } else {
        "127.0.0.1"
    };
    let key = format!("{host}:{port}");
    let mut children = CHILDREN.lock().map_err(|_| "Démarrage indisponible.")?;
    if cancel.is_cancelled() {
        return Err("Démarrage annulé.".into());
    }
    if let Some(child) = children.get_mut(&key)
        && child.try_wait().map_err(|e| e.to_string())?.is_none()
    {
        return Ok(());
    }
    let (root, python) = resolve(&config)?;
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join("voicestudio.log"))
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(python);
    command
        .current_dir(root)
        .args([
            "-m",
            "uvicorn",
            "main:app",
            "--app-dir",
            "backend",
            "--host",
            host,
            "--port",
        ])
        .arg(port.to_string())
        .stdin(Stdio::null())
        .stdout(log.try_clone().map_err(|e| e.to_string())?)
        .stderr(log)
        .env_remove("PYTHONHOME")
        .env_remove("PYTHONPATH")
        .env_remove("OMNIVOICE_DESKTOP_CONTAINED")
        .env_remove("OMNIVOICE_DESKTOP_DRAIN_FD")
        .env("PYTHONUTF8", "1")
        .env("PYTHONUNBUFFERED", "1")
        .env("OMNIVOICE_PORT", port.to_string())
        .env("TORCHDYNAMO_DISABLE", "1");
    storage_environment(&mut command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    if cancel.is_cancelled() {
        return Err("Démarrage annulé.".into());
    }
    let mut process = Process::spawn(&mut command)?;
    if cancel.is_cancelled() {
        process.stop();
        return Err("Démarrage annulé.".into());
    }
    children.insert(key, process);
    Ok(())
}
pub fn shutdown() {
    let mut children = CHILDREN.lock().unwrap_or_else(|e| e.into_inner());
    for child in children.values_mut() {
        child.stop();
    }
    children.clear();
}

#[cfg(test)]
#[path = "service_process_tests.rs"]
mod tests;
