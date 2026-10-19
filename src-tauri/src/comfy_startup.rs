//! Optional device-local ComfyUI startup. Imported companions never supply a
//! command. Only Python/portable installations explicitly selected on this PC
//! can start, bound to the configured loopback address and port.
use crate::owned_process::Process as Child;
use reqwest::Client;
use serde::Serialize;
use sherka_core::model::ComfyStartupConfig;
use std::{
    collections::HashMap,
    fs::OpenOptions,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};
use tauri::{Emitter, Manager};
use tokio::sync::watch;
use tokio_util::sync::CancellationToken;

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub attempt: u64,
    pub url: String,
    pub phase: &'static str,
    pub message: String,
}
struct Attempt {
    id: u64,
    key: String,
    cancel: CancellationToken,
}
type Launcher = Arc<
    dyn Fn(&ComfyStartupConfig, &url::Url, &CancellationToken) -> Result<Option<Child>, String>
        + Send
        + Sync,
>;
type Reporter = Arc<dyn Fn(Status) + Send + Sync>;
struct Inner {
    client: Client,
    attempt: Mutex<Attempt>,
    status: watch::Sender<Status>,
    cancel: CancellationToken,
    launch: Launcher,
    report: Reporter,
    budget: Duration,
    processes: Mutex<HashMap<String, Child>>,
}
#[derive(Clone)]
pub struct ComfyStartup(Arc<Inner>);

pub fn local_url(value: &str) -> Option<url::Url> {
    let url = sherka_core::illustration::server_url(value).ok()?;
    let local = match url.host()? {
        url::Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(ip) => ip.is_loopback(),
        url::Host::Ipv6(ip) => ip.is_loopback(),
    };
    (local && url.scheme() == "http").then_some(url)
}

impl ComfyStartup {
    pub fn new(app: tauri::AppHandle, root: PathBuf) -> Result<Self, String> {
        Self::build(
            Arc::new(move |config, url, token| launch(config, url, &root, token)),
            Arc::new(move |status| {
                let _ = app.emit("sherka:comfy-service", status);
            }),
            Duration::from_secs(300),
        )
    }
    fn build(launch: Launcher, report: Reporter, budget: Duration) -> Result<Self, String> {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(2))
            .timeout(Duration::from_secs(3))
            .build()
            .map_err(|e| e.to_string())?;
        let (status, _) = watch::channel(Status {
            attempt: 0,
            url: String::new(),
            phase: "disabled",
            message: String::new(),
        });
        Ok(Self(Arc::new(Inner {
            client,
            attempt: Mutex::new(Attempt {
                id: 0,
                key: String::new(),
                cancel: CancellationToken::new(),
            }),
            status,
            cancel: CancellationToken::new(),
            launch,
            report,
            budget,
            processes: Mutex::new(HashMap::new()),
        })))
    }
    pub fn status(&self) -> Status {
        self.0.status.borrow().clone()
    }
    pub fn shutdown(&self) {
        self.cancel();
        let mut children = self.0.processes.lock().unwrap_or_else(|e| e.into_inner());
        for child in children.values_mut() {
            child.stop();
        }
        children.clear();
    }
    pub fn cancel(&self) {
        self.0.cancel.cancel();
        self.disable();
    }
    pub fn disable(&self) {
        let mut attempt = self.0.attempt.lock().unwrap_or_else(|e| e.into_inner());
        attempt.cancel.cancel();
        attempt.id += 1;
        attempt.key.clear();
        let status = Status {
            attempt: attempt.id,
            url: String::new(),
            phase: "disabled",
            message: String::new(),
        };
        self.0.status.send_replace(status.clone());
        (self.0.report)(status);
    }
    fn update(&self, id: u64, phase: &'static str, message: String) {
        let attempt = self.0.attempt.lock().unwrap_or_else(|e| e.into_inner());
        if attempt.id != id || attempt.cancel.is_cancelled() || self.0.cancel.is_cancelled() {
            return;
        }
        let status = Status {
            attempt: id,
            url: self.0.status.borrow().url.clone(),
            phase,
            message,
        };
        self.0.status.send_replace(status.clone());
        (self.0.report)(status);
    }
    pub fn start(
        &self,
        base: &str,
        config: &ComfyStartupConfig,
        recheck: bool,
    ) -> Result<u64, String> {
        if config.directory.trim().is_empty() || local_url(base).is_none() {
            return Ok(0);
        }
        if config.disabled {
            return Ok(0);
        }
        if self.0.cancel.is_cancelled() {
            return Err("Démarrage de ComfyUI annulé.".into());
        }
        let url = local_url(base).unwrap();
        let key = format!("{}\n{}\n{}", url, config.directory, config.python);
        let mut attempt = self
            .0
            .attempt
            .lock()
            .map_err(|_| "Démarrage de ComfyUI indisponible.")?;
        let phase = self.0.status.borrow().phase;
        if attempt.key == key
            && !attempt.cancel.is_cancelled()
            && (!recheck || matches!(phase, "connecting" | "starting"))
        {
            return Ok(attempt.id);
        }
        attempt.cancel.cancel();
        attempt.id += 1;
        attempt.key = key;
        attempt.cancel = CancellationToken::new();
        let id = attempt.id;
        let token = attempt.cancel.clone();
        let status = Status {
            attempt: id,
            url: url.to_string(),
            phase: "connecting",
            message: "Connexion à ComfyUI…".into(),
        };
        self.0.status.send_replace(status.clone());
        (self.0.report)(status);
        drop(attempt);
        let service = self.clone();
        let config = config.clone();
        tauri::async_runtime::spawn(async move {
            let result = tokio::select! {biased;_=token.cancelled()=>return,_=service.0.cancel.cancelled()=>return,result=tokio::time::timeout(service.0.budget,service.run(id,url,config,token.clone()))=>result.unwrap_or_else(|_|Err("ComfyUI n’est pas prêt après 5 minutes. Vérifie son chemin, son environnement Python et son journal.".into()))};
            match result {
                Ok(()) => service.update(id, "ready", "ComfyUI est prêt.".into()),
                Err(e) => service.update(id, "error", e),
            }
        });
        Ok(id)
    }
    pub async fn ensure(
        &self,
        base: &str,
        config: &ComfyStartupConfig,
        token: &CancellationToken,
        recheck: bool,
    ) -> Result<(), String> {
        if token.is_cancelled() {
            return Err("Démarrage de ComfyUI annulé.".into());
        }
        let id = self.start(base, config, recheck)?;
        if id == 0 {
            return Ok(());
        }
        let mut state = self.0.status.subscribe();
        loop {
            let status = state.borrow().clone();
            if status.attempt != id {
                return Err("Démarrage de ComfyUI annulé.".into());
            }
            match status.phase {
                "ready" => return Ok(()),
                "error" => return Err(status.message),
                _ => {}
            }
            tokio::select! {biased;_=token.cancelled()=>return Err("Démarrage de ComfyUI annulé.".into()),_=self.0.cancel.cancelled()=>return Err("Démarrage de ComfyUI annulé.".into()),result=state.changed()=>{result.map_err(|_|"Démarrage de ComfyUI indisponible.")?;}}
        }
    }
    async fn probe(&self, url: &url::Url) -> Result<Option<bool>, String> {
        let mut response = match self
            .0
            .client
            .get(url.join("system_stats").unwrap())
            .send()
            .await
        {
            Ok(value) => value,
            Err(e) if e.is_connect() => return Ok(Some(false)),
            Err(e) if e.is_timeout() => return Ok(None),
            Err(_) => return Err("Impossible de joindre ComfyUI.".into()),
        };
        if !response.status().is_success() {
            return Err(format!(
                "ComfyUI est inaccessible (HTTP {}). Vérifie l’adresse et les autorisations.",
                response.status().as_u16()
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| "Réponse ComfyUI interrompue.")?
        {
            if bytes.len() + chunk.len() > 128 * 1024 {
                return Err("Cette adresse ne renvoie pas un serveur ComfyUI compatible.".into());
            }
            bytes.extend_from_slice(&chunk);
        }
        let body: serde_json::Value = serde_json::from_slice(&bytes)
            .map_err(|_| "Cette adresse ne renvoie pas un serveur ComfyUI compatible.")?;
        if body["system"]["comfyui_version"].as_str().is_none() || !body["devices"].is_array() {
            return Err("Cette adresse ne renvoie pas un serveur ComfyUI compatible.".into());
        }
        Ok(Some(true))
    }
    async fn run(
        &self,
        id: u64,
        url: url::Url,
        config: ComfyStartupConfig,
        token: CancellationToken,
    ) -> Result<(), String> {
        let mut started = false;
        let key = format!("{}\n{}\n{}", url, config.directory, config.python);
        loop {
            if token.is_cancelled() || self.0.cancel.is_cancelled() {
                return Err("Démarrage de ComfyUI annulé.".into());
            }
            match self.probe(&url).await? {
                Some(true) => return Ok(()),
                Some(false) if !started => {
                    self.update(id,"starting","ComfyUI démarre. Les premières générations peuvent prendre plus de temps pendant le chargement des modèles.".into());
                    let (service, config, url, token, key) = (
                        self.clone(),
                        config.clone(),
                        url.clone(),
                        token.clone(),
                        key.clone(),
                    );
                    tauri::async_runtime::spawn_blocking(move || {
                        let mut processes = service
                            .0
                            .processes
                            .lock()
                            .map_err(|_| "Démarrage de ComfyUI indisponible.")?;
                        if let Some(process) = processes.get_mut(&key)
                            && process.try_wait().map_err(|e| e.to_string())?.is_none()
                        {
                            return Ok::<_, String>(());
                        }
                        if token.is_cancelled() {
                            return Err("Démarrage de ComfyUI annulé.".into());
                        }
                        if let Some(child) = (service.0.launch)(&config, &url, &token)? {
                            let mut child = child;
                            if token.is_cancelled() || service.0.cancel.is_cancelled() {
                                child.stop();
                            } else {
                                processes.insert(key, child);
                            }
                        }
                        Ok(())
                    })
                    .await
                    .map_err(|_| "Impossible de démarrer ComfyUI.")??;
                    started = true;
                }
                _ => {}
            }
            if let Some(process) = self
                .0
                .processes
                .lock()
                .map_err(|_| "Démarrage de ComfyUI indisponible.")?
                .get_mut(&key)
                && process.try_wait().map_err(|e| e.to_string())?.is_some()
            {
                return Err("ComfyUI s’est arrêté pendant le démarrage. Vérifie Python et le journal logs/comfyui.log dans les données de Sherka.".into());
            }
            tokio::select! {biased;_=token.cancelled()=>return Err("Démarrage de ComfyUI annulé.".into()),_=tokio::time::sleep(Duration::from_millis(650))=>{}}
        }
    }
}

fn launch_command(
    config: &ComfyStartupConfig,
    url: &url::Url,
    root: &Path,
) -> Result<Command, String> {
    let selected = Path::new(config.directory.trim());
    if !selected.is_absolute() {
        return Err("Choisis le chemin absolu du dossier ComfyUI.".into());
    }
    let selected = selected
        .canonicalize()
        .map_err(|_| "Le dossier ComfyUI est introuvable.")?;
    let folder = if selected.join("main.py").is_file() {
        selected.clone()
    } else {
        selected.join("ComfyUI")
    };
    if !folder.join("main.py").is_file() {
        return Err("Ce dossier ne contient pas main.py. Choisis une installation Python ou portable de ComfyUI.".into());
    }
    let mut candidates = vec![];
    #[cfg(windows)]
    for base in [&folder, &selected] {
        candidates.extend([
            base.join(".venv/Scripts/python.exe"),
            base.join("venv/Scripts/python.exe"),
            base.join("python_embeded/python.exe"),
        ]);
    }
    #[cfg(not(windows))]
    for base in [&folder, &selected] {
        candidates.extend([base.join(".venv/bin/python"), base.join("venv/bin/python")]);
    }
    #[cfg(windows)]
    if let Some(parent) = folder.parent() {
        candidates.push(parent.join("python_embeded/python.exe"));
    }
    let python = if !config.python.trim().is_empty() {
        let path = PathBuf::from(config.python.trim());
        if !path.is_absolute() || !path.is_file() {
            return Err("Le chemin de Python est introuvable. Choisis le Python de ton environnement ComfyUI.".into());
        }
        path
    } else {
        candidates
            .into_iter()
            .find(|p| p.is_file())
            .unwrap_or_else(|| PathBuf::from(if cfg!(windows) { "python" } else { "python3" }))
    };
    let host = match url.host() {
        Some(url::Host::Ipv6(_)) => "::1".to_owned(),
        Some(url::Host::Ipv4(ip)) => ip.to_string(),
        _ => "127.0.0.1".into(),
    };
    let logs = root.join("logs");
    std::fs::create_dir_all(&logs).map_err(|e| e.to_string())?;
    let log = OpenOptions::new()
        .create(true)
        .append(true)
        .open(logs.join("comfyui.log"))
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(python);
    command
        .current_dir(&folder)
        .args([
            "-s",
            "main.py",
            "--listen",
            &host,
            "--port",
            &url.port_or_known_default().unwrap_or(8188).to_string(),
            "--disable-auto-launch",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .stdout(Stdio::from(log));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    Ok(command)
}

fn launch(
    config: &ComfyStartupConfig,
    url: &url::Url,
    root: &Path,
    cancel: &CancellationToken,
) -> Result<Option<Child>, String> {
    let mut command = launch_command(config, url, root)?;
    if cancel.is_cancelled() {
        return Err("Démarrage de ComfyUI annulé.".into());
    }
    Child::spawn(&mut command)
        .map(Some)
        .map_err(|e| format!("Impossible de démarrer ComfyUI : {e}"))
}

#[tauri::command]
pub fn get_comfy_service(state: tauri::State<'_, ComfyStartup>) -> Status {
    state.status()
}

#[tauri::command]
pub async fn select_comfyui_path(
    app: tauri::AppHandle,
    python: bool,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let dialog = app.dialog().file();
        let picked = if python {
            dialog.blocking_pick_file()
        } else {
            dialog.blocking_pick_folder()
        };
        picked
            .map(|file| {
                file.into_path()
                    .map(|path| path.display().to_string())
                    .map_err(|e| e.to_string())
            })
            .transpose()
    })
    .await
    .map_err(|_| "Impossible d’ouvrir le sélecteur de fichiers.".to_owned())?
}

pub fn synchronize(app: &tauri::AppHandle, shared: &crate::Shared) {
    let Some(service) = app.try_state::<ComfyStartup>() else {
        return;
    };
    let Ok(core) = shared.0.lock() else {
        return;
    };
    let Ok(profile) = core.profile(core.session.snapshot().profile_id) else {
        return;
    };
    let config = core.comfy_startup_config();
    drop(core);
    if profile.details.illustration.enabled
        && !profile.details.connections.llm_provider.is_agent()
        && !config.disabled
        && !config.directory.is_empty()
        && local_url(&profile.details.illustration.server_url).is_some()
    {
        let _ = service.start(&profile.details.illustration.server_url, &config, false);
    } else if service.status().phase != "disabled" {
        service.disable();
    }
}

#[cfg(test)]
#[path = "comfy_startup_tests.rs"]
mod tests;
