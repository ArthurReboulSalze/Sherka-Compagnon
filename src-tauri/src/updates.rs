use serde::Serialize;
use std::{sync::Mutex, time::Duration};
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_updater::{Update, UpdaterExt};
use url::Url;

const REPOSITORY: &str = "ArthurReboulSalze/Sherka-Compagnon";
const MANIFEST: &str =
    "https://github.com/ArthurReboulSalze/Sherka-Compagnon/releases/latest/download/latest.json";

#[derive(Clone, Serialize)]
pub struct Status {
    current_version: &'static str,
    mode: &'static str,
    phase: &'static str,
    version: Option<String>,
    notes: Option<String>,
    downloaded: u64,
    total: Option<u64>,
    message: String,
}

struct Inner {
    status: Status,
    update: Option<Update>,
    busy: bool,
}

pub struct Updates(Mutex<Inner>);

impl Updates {
    pub fn new() -> Self {
        let installed = installed_here();
        Self(Mutex::new(Inner {
            status: Status {
                current_version: env!("CARGO_PKG_VERSION"),
                mode: if installed { "installed" } else { "portable" },
                phase: "idle",
                version: None,
                notes: None,
                downloaded: 0,
                total: None,
                message: "Vérifie les nouvelles versions sur GitHub quand tu le souhaites.".into(),
            },
            update: None,
            busy: false,
        }))
    }

    fn publish(&self, app: &AppHandle, phase: &'static str, message: impl Into<String>) {
        let mut inner = self.0.lock().unwrap();
        inner.status.phase = phase;
        inner.status.message = message.into();
        let _ = app.emit("sherka:update", &inner.status);
    }

    fn begin(&self) -> Result<(), String> {
        crate::windows::ensure_running()?;
        let mut inner = self.0.lock().unwrap();
        if inner.busy {
            return Err("Une vérification ou une mise à jour est déjà en cours.".into());
        }
        inner.busy = true;
        Ok(())
    }

    fn finish(&self, app: &AppHandle, result: Result<(), String>) -> Result<Status, String> {
        self.0.lock().unwrap().busy = false;
        if let Err(error) = result {
            self.publish(app, "error", &error);
            return Err(error);
        }
        Ok(self.0.lock().unwrap().status.clone())
    }
}

#[tauri::command]
pub fn get_update_status(state: State<'_, Updates>) -> Status {
    state.0.lock().unwrap().status.clone()
}

#[tauri::command]
pub async fn check_update(app: AppHandle, state: State<'_, Updates>) -> Result<Status, String> {
    state.begin()?;
    state.0.lock().unwrap().update = None;
    state.publish(
        &app,
        "checking",
        "Recherche d’une nouvelle version sur GitHub…",
    );
    let result = async {
        let mode = state.0.lock().unwrap().status.mode;
        let endpoint = manifest_url()?;
        let builder = app
            .updater_builder()
            .target(if mode == "portable" {
                "windows-x86_64-portable"
            } else {
                "windows-x86_64-nsis"
            })
            .timeout(Duration::from_secs(180))
            .endpoints(vec![endpoint.clone()])
            .map_err(|e| e.to_string())?;
        let builder = if cfg!(debug_assertions)
            && endpoint.scheme() == "http"
            && std::env::var("SHERKA_TEST_ALLOW_CURRENT_UPDATE").as_deref() == Ok("1")
        {
            builder.version_comparator(|current, remote| remote.version >= current)
        } else {
            builder
        };
        let updater = builder.build().map_err(|e| e.to_string())?;
        let update = tokio::time::timeout(Duration::from_secs(25), updater.check())
            .await
            .map_err(|_| "GitHub ne répond pas. Tu peux réessayer plus tard.".to_string())?
            .map_err(|_| {
                "Impossible de vérifier GitHub. Vérifie ta connexion et réessaie.".to_string()
            })?;
        if let Some(update) = &update {
            validate_asset(&update.download_url, &update.version, mode, &endpoint)?;
        }
        {
            let mut inner = state.0.lock().unwrap();
            inner.status.version = update.as_ref().map(|u| u.version.clone());
            inner.status.notes = update.as_ref().and_then(|u| u.body.clone());
            inner.status.downloaded = 0;
            inner.status.total = None;
            inner.update = update;
        }
        let version = state.0.lock().unwrap().status.version.clone();
        if let Some(version) = version {
            state.publish(
                &app,
                "available",
                format!("Sherka {version} est disponible."),
            );
        } else {
            state.publish(
                &app,
                "current",
                "Tu utilises la dernière version de Sherka.",
            );
        }
        Ok(())
    }
    .await;
    state.finish(&app, result)
}

#[tauri::command]
pub async fn install_update(app: AppHandle, state: State<'_, Updates>) -> Result<Status, String> {
    state.begin()?;
    let result = async {
        let (update, mode) = {
            let inner = state.0.lock().unwrap();
            (inner.update.clone().ok_or("Vérifie d’abord les mises à jour.")?, inner.status.mode)
        };
        state.publish(&app, "downloading", "Téléchargement et vérification de la mise à jour…");
        let mut last_event = std::time::Instant::now();
        let bytes = update.download(|chunk, total| {
            let mut inner = state.0.lock().unwrap();
            inner.status.downloaded += chunk as u64;
            inner.status.total = total;
            if last_event.elapsed() > Duration::from_millis(100) {
                let _ = app.emit("sherka:update", &inner.status);
                last_event = std::time::Instant::now();
            }
        }, || {}).await.map_err(|_| "La mise à jour n’a pas pu être téléchargée ou sa signature est invalide. Aucune installation effectuée.".to_string())?;
        // Downloads are verified by Tauri before any shutdown or file change.
        let mut portable = if mode == "portable" {
            Some(crate::portable_update::prepare(&app, &bytes, &update.signature, &update.version).await?)
        } else { None };
        state.publish(&app, "installing", "Préparation du redémarrage · arrêt de la voix et déchargement du modèle…");
        if let Err(error) = crate::windows::prepare_shutdown(&app).await {
            if let Some(helper) = &mut portable { helper.cancel(); }
            return Err(format!("Mise à jour suspendue : {error}"));
        }
        if let Some(helper) = portable {
            helper.commit();
            app.exit(0);
        } else if let Err(error) = update.install(&bytes) {
            crate::windows::update_failed();
            return Err(format!("L’installation n’a pas démarré : {error}"));
        }
        Ok(())
    }.await;
    state.finish(&app, result)
}

fn manifest_url() -> Result<Url, String> {
    // A loopback-only override exists exclusively in debug builds with isolated data.
    if cfg!(debug_assertions)
        && std::env::var_os("SHERKA_DATA_DIR").is_some()
        && let Ok(value) = std::env::var("SHERKA_TEST_UPDATE_URL")
    {
        let url = Url::parse(&value).map_err(|e| e.to_string())?;
        if url.scheme() == "http" && url.host_str() == Some("127.0.0.1") {
            return Ok(url);
        }
        return Err("Le serveur de test doit être local.".into());
    }
    Url::parse(MANIFEST).map_err(|e| e.to_string())
}

fn validate_asset(url: &Url, version: &str, mode: &str, endpoint: &Url) -> Result<(), String> {
    let name = if mode == "portable" {
        "sherka-compagnon.exe".into()
    } else {
        format!("Sherka-Compagnon_{version}_x64-setup.exe")
    };
    let path = format!("/{REPOSITORY}/releases/download/v{version}/{name}");
    let test = cfg!(debug_assertions)
        && endpoint.scheme() == "http"
        && endpoint.host_str() == Some("127.0.0.1")
        && url.origin() == endpoint.origin();
    if (!test && (url.scheme() != "https" || url.host_str() != Some("github.com")))
        || url.path() != path
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err("Cette mise à jour ne provient pas de la release officielle Sherka.".into());
    }
    Ok(())
}

fn installed_here() -> bool {
    #[cfg(windows)]
    {
        use winreg::{
            RegKey,
            enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE},
        };
        let Ok(exe) = std::env::current_exe().and_then(std::fs::canonicalize) else {
            return false;
        };
        for hive in [HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE] {
            if let Ok(key) = RegKey::predef(hive).open_subkey(
                "Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Sherka Compagnon",
            ) && let Ok(location) = key.get_value::<String, _>("InstallLocation")
                && let Ok(installed) = std::fs::canonicalize(
                    std::path::Path::new(location.trim_matches('"')).join("sherka-compagnon.exe"),
                )
                && installed == exe
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_official_release_assets_are_accepted() {
        let endpoint = Url::parse(MANIFEST).unwrap();
        let asset = format!(
            "https://github.com/{REPOSITORY}/releases/download/v0.3.4/sherka-compagnon.exe"
        );
        assert!(
            validate_asset(&Url::parse(&asset).unwrap(), "0.3.4", "portable", &endpoint).is_ok()
        );
        for bad in [
            asset.replace("github.com", "example.com"),
            asset.replace(REPOSITORY, "other/repo"),
            format!("{asset}?token=secret"),
            asset.replace("v0.3.4/", "v0.3.5/"),
        ] {
            assert!(
                validate_asset(&Url::parse(&bad).unwrap(), "0.3.4", "portable", &endpoint).is_err()
            );
        }
        assert!(
            validate_asset(
                &Url::parse(&asset).unwrap(),
                "0.3.4",
                "installed",
                &endpoint
            )
            .is_err()
        );
    }
}
