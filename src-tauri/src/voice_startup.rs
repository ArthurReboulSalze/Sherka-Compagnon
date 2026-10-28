//! VoiceStudio is the required speech service. Conversation providers do not
//! participate in this lifecycle, and no microphone is opened here.
use reqwest::{Client, StatusCode};
use serde::Serialize;
use sherka_core::catalogs::{CatalogClient, DiscoveryError, VoiceCatalog};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
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

type Launcher = Arc<dyn Fn(u16, CancellationToken) -> Result<(), String> + Send + Sync>;
type Reporter = Arc<dyn Fn(Status) + Send + Sync>;
// Exit code of the server this session started on a port, once it has died.
type Exited = Arc<dyn Fn(u16) -> Option<String> + Send + Sync>;
struct Attempt {
    id: u64,
    cancel: CancellationToken,
}
struct Inner {
    client: Client,
    attempt: Mutex<Attempt>,
    state: watch::Sender<Status>,
    cancel: CancellationToken,
    launch: Launcher,
    exited: Exited,
    report: Reporter,
    enabled: bool,
    budget: Duration,
    interval: Duration,
}

#[derive(Clone)]
pub struct VoiceStartup(Arc<Inner>);

impl VoiceStartup {
    pub fn new(app: tauri::AppHandle) -> Result<Self, String> {
        let owner = app.clone();
        Self::build(
            Arc::new(move |port, token| {
                let shared = owner.state::<crate::Shared>();
                let core = shared.0.lock().map_err(|_| "Démarrage indisponible.")?;
                let config = core.voice_service_config();
                let logs = core.store.root().join("logs");
                drop(core);
                crate::service_process::launch(port, token, config, logs)
            }),
            Arc::new(crate::service_process::exited),
            Arc::new(move |status| {
                let _ = app.emit("sherka:voice-service", status);
            }),
            std::env::var("SHERKA_AUTOSTART_VOICE").as_deref() != Ok("0"),
            // A cold start can load several large speech models.
            Duration::from_secs(600),
            Duration::from_millis(700),
        )
    }

    fn build(
        launch: Launcher,
        exited: Exited,
        report: Reporter,
        enabled: bool,
        budget: Duration,
        interval: Duration,
    ) -> Result<Self, String> {
        let client = Client::builder()
            .no_proxy()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(Duration::from_secs(4))
            .timeout(Duration::from_secs(5))
            .build()
            .map_err(|e| e.to_string())?;
        let (state, _) = watch::channel(Status {
            attempt: 0,
            url: String::new(),
            phase: "disabled",
            message: String::new(),
        });
        Ok(Self(Arc::new(Inner {
            client,
            attempt: Mutex::new(Attempt {
                id: 0,
                cancel: CancellationToken::new(),
            }),
            state,
            cancel: CancellationToken::new(),
            launch,
            exited,
            report,
            enabled,
            budget,
            interval,
        })))
    }

    pub fn status(&self) -> Status {
        self.0.state.borrow().clone()
    }
    pub fn reset(&self) {
        let mut attempt = self.0.attempt.lock().unwrap_or_else(|e| e.into_inner());
        attempt.cancel.cancel();
        attempt.id += 1;
        self.publish(Status {
            attempt: attempt.id,
            url: String::new(),
            phase: "disabled",
            message: String::new(),
        });
    }
    pub fn cancel(&self) {
        self.0.cancel.cancel();
    }

    // One shared attempt per endpoint: startup, retry and speech requests join
    // the same worker. Changing the endpoint cancels the previous wait.
    pub fn start(&self, url: &str, recheck: bool) -> Result<u64, String> {
        if self.0.cancel.is_cancelled() {
            return Err("Démarrage de VoiceStudio annulé.".into());
        }
        if !self.0.enabled {
            return Ok(0);
        }
        let url = sherka_core::connections::base_url(url).map_err(|e| e.to_string())?;
        let mut attempt = self
            .0
            .attempt
            .lock()
            .map_err(|_| "Démarrage indisponible.")?;
        let status = self.status();
        if status.url == url
            && (matches!(status.phase, "checking" | "starting")
                || (!recheck && status.phase == "ready"))
        {
            return Ok(attempt.id);
        }
        attempt.cancel.cancel();
        attempt.id += 1;
        attempt.cancel = self.0.cancel.child_token();
        let id = attempt.id;
        let cancel = attempt.cancel.clone();
        self.publish(Status {
            attempt: id,
            url: url.clone(),
            phase: "checking",
            message: "Connexion à VoiceStudio…".into(),
        });
        let service = self.clone();
        tauri::async_runtime::spawn(async move {
            let result = tokio::select! {
                biased;
                _ = cancel.cancelled() => return,
                result = tokio::time::timeout(service.0.budget, service.prepare(id, &url, cancel.clone())) =>
                    result.unwrap_or_else(|_| Err(format!("VoiceStudio n’est pas prêt après {} min. Il charge peut-être encore ses modèles : consulte son journal (logs/voicestudio.log), puis réessaie.", (service.0.budget.as_secs() / 60).max(1)))),
            };
            match result {
                Ok(()) => service.update(id, "ready", "VoiceStudio est prêt.".into()),
                Err(error) => service.update(id, "error", error),
            }
        });
        Ok(id)
    }

    pub async fn ensure(&self, url: &str) -> Result<(), String> {
        let mut changes = self.0.state.subscribe();
        let id = self.start(url, true)?;
        if !self.0.enabled {
            return Ok(());
        }
        loop {
            let status = changes.borrow_and_update().clone();
            if status.attempt != id {
                return Err(
                    "La connexion vocale a changé. Réessaie avec le compagnon actif.".into(),
                );
            }
            match status.phase {
                "ready" => return Ok(()),
                "error" => return Err(status.message),
                _ => {}
            }
            tokio::select! {
                biased;
                _ = self.0.cancel.cancelled() => return Err("Démarrage de VoiceStudio annulé.".into()),
                result = changes.changed() => { result.map_err(|_| "Démarrage indisponible.")?; }
            }
        }
    }

    fn publish(&self, status: Status) {
        self.0.state.send_replace(status.clone());
        (self.0.report)(status);
    }
    fn update(&self, id: u64, phase: &'static str, message: String) {
        let Ok(attempt) = self.0.attempt.lock() else {
            return;
        };
        if attempt.id != id || attempt.cancel.is_cancelled() {
            return;
        }
        let mut status = self.status();
        if status.phase == phase && status.message == message {
            return;
        }
        status.phase = phase;
        status.message = message;
        self.publish(status);
    }

    // A server that is still alive or still answering "not ready" is only
    // slow: it stays `starting` until it is ready, dies, or the budget ends.
    async fn prepare(&self, id: u64, url: &str, cancel: CancellationToken) -> Result<(), String> {
        let started = Instant::now();
        let mut launched = None;
        loop {
            match probe(&self.0.client, url).await? {
                Probe::Ready => return Ok(()),
                Probe::Stopped if launched.is_none() => {
                    let port = local_port(url).ok_or("VoiceStudio est inaccessible à cette adresse. Le lancement automatique nécessite une adresse HTTP locale, sans chemin intermédiaire.")?;
                    self.update(id, "starting", "Démarrage de VoiceStudio…".into());
                    let launch = self.0.launch.clone();
                    let cancel = cancel.clone();
                    tauri::async_runtime::spawn_blocking(move || launch(port, cancel))
                        .await
                        .map_err(|_| "Impossible de démarrer VoiceStudio.")??;
                    launched = Some(port);
                    tokio::time::sleep(self.0.interval).await;
                    continue;
                }
                Probe::Stopped | Probe::Pending => {}
            }
            if let Some(code) = launched.and_then(|port| (self.0.exited)(port)) {
                return Err(format!(
                    "VoiceStudio s’est arrêté pendant son démarrage (code {}). Consulte son journal (logs/voicestudio.log), puis réessaie.",
                    code
                ));
            }
            self.update(
                id,
                "starting",
                format!(
                    "VoiceStudio démarre et charge ses modèles… ({} s)",
                    started.elapsed().as_secs()
                ),
            );
            tokio::time::sleep(self.0.interval).await;
        }
    }
}

// Do not launch a local program for a remote server, HTTPS proxy, or API path.
fn local_port(base: &str) -> Option<u16> {
    let url = url::Url::parse(base).ok()?;
    let local = match url.host()? {
        url::Host::Domain(name) => name.eq_ignore_ascii_case("localhost"),
        url::Host::Ipv4(ip) => ip == std::net::Ipv4Addr::LOCALHOST,
        url::Host::Ipv6(ip) => ip == std::net::Ipv6Addr::LOCALHOST,
    };
    (local
        && url.scheme() == "http"
        && url.path() == "/"
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none())
    .then(|| url.port_or_known_default())
    .flatten()
}

enum Probe {
    Ready,
    Stopped,
    Pending,
}
// `None`: the body was cut off or timed out, which a loading server does.
async fn json(mut response: reqwest::Response) -> Result<Option<serde_json::Value>, String> {
    let mut bytes = Vec::new();
    loop {
        match response.chunk().await {
            Ok(Some(chunk)) if bytes.len() + chunk.len() > 65_536 => {
                return Err("Réponse VoiceStudio trop volumineuse.".into());
            }
            Ok(Some(chunk)) => bytes.extend_from_slice(&chunk),
            Ok(None) => break,
            Err(_) => return Ok(None),
        }
    }
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|_| "Cette adresse ne renvoie pas une réponse VoiceStudio valide.".into())
}
fn checked(response: &reqwest::Response) -> Result<(), String> {
    if response.status().is_success() {
        Ok(())
    } else {
        Err(format!(
            "VoiceStudio est inaccessible (HTTP {}). Vérifie son adresse et son accès dans les réglages.",
            response.status().as_u16()
        ))
    }
}
// Server errors are what a Python server answers while it loads its models.
fn loading(status: reqwest::StatusCode) -> bool {
    status.is_server_error()
}
fn announces_loading(health: &serde_json::Value) -> bool {
    health["ready"] == false
        || health["status"].as_str().is_some_and(|status| {
            let status = status.to_ascii_lowercase();
            [
                "start",
                "load",
                "init",
                "warm",
                "boot",
                "not_ready",
                "not ready",
            ]
            .iter()
            .any(|word| status.contains(word))
        })
}
async fn probe(client: &Client, base: &str) -> Result<Probe, String> {
    let health = match client.get(format!("{base}/health")).send().await {
        Ok(response) => response,
        Err(error) if error.is_timeout() => return Ok(Probe::Pending),
        Err(error) if error.is_connect() => return Ok(Probe::Stopped),
        Err(error) if error.is_builder() || error.is_redirect() => {
            return Err("Impossible de joindre VoiceStudio. Vérifie son adresse.".into());
        }
        // A reset or closed connection: the server is busy starting.
        Err(_) => return Ok(Probe::Pending),
    };
    if loading(health.status()) {
        return Ok(Probe::Pending);
    }
    checked(&health)?;
    let Some(health) = json(health).await? else {
        return Ok(Probe::Pending);
    };
    if announces_loading(&health) {
        return Ok(Probe::Pending);
    }
    if health["status"] != "ok" || !health["version"].is_string() || !health["device"].is_string() {
        return Err("Un autre service répond à cette adresse, ou VoiceStudio n’est pas compatible. Vérifie l’adresse dans les réglages.".into());
    }
    let response = match client
        .get(format!("{base}/.well-known/voicestudio-speech"))
        .send()
        .await
    {
        Ok(response) => response,
        Err(error) if error.is_builder() || error.is_redirect() => {
            return Err("Impossible de vérifier les capacités de VoiceStudio.".into());
        }
        Err(_) => return Ok(Probe::Pending),
    };
    if loading(response.status()) {
        return Ok(Probe::Pending);
    }
    checked(&response)?;
    let Some(capabilities) = json(response).await? else {
        return Ok(Probe::Pending);
    };
    if capabilities["protocol"] != "voicestudio.speech.v1" {
        return Err(
            "VoiceStudio ne fournit pas le protocole vocal attendu. Vérifie sa version.".into(),
        );
    }
    Ok(Probe::Ready)
}

/// How long the voice catalog is retried after VoiceStudio reported ready.
const CATALOG_PATIENCE: Duration = Duration::from_secs(20);

/// What to do about a failed voice catalog read: wait this long and try again
/// (`waited`: time spent since the first try), or `None` to report the error.
/// Only what a server finishing its start-up does is worth another try
/// (unreachable, timeout, 5xx, as for `loading`); a missing voice or any 4xx,
/// an authentication or a format problem is final.
fn catalog_retry(error: &DiscoveryError, attempt: u32, waited: Duration) -> Option<Duration> {
    let transient = match error {
        DiscoveryError::Unreachable | DiscoveryError::Timeout => true,
        DiscoveryError::Http(code, _) => StatusCode::from_u16(*code).is_ok_and(loading),
        _ => false,
    };
    (transient && waited < CATALOG_PATIENCE)
        .then(|| Duration::from_millis(250u64 << attempt.min(3)).min(CATALOG_PATIENCE - waited))
}

/// The voice catalog right after `ensure` succeeded: VoiceStudio may still be
/// loading its models, so transient failures are retried for a few seconds.
/// `cancel` is the turn's pending token.
pub async fn voice_catalog(url: &str, cancel: &CancellationToken) -> Result<VoiceCatalog, String> {
    let client = CatalogClient::new(url).map_err(|e| e.to_string())?;
    let started = Instant::now();
    let mut attempt = 0;
    loop {
        let error = match client.voices().await {
            Ok(catalog) => return Ok(catalog),
            Err(error) => error,
        };
        let Some(wait) = catalog_retry(&error, attempt, started.elapsed()) else {
            return Err(error.to_string());
        };
        attempt += 1;
        tokio::select! {
            biased;
            _ = cancel.cancelled() => return Err("Demande vocale annulée.".into()),
            _ = tokio::time::sleep(wait) => {}
        }
    }
}

pub async fn start_active(app: &tauri::AppHandle, recheck: bool) -> Result<(), String> {
    crate::windows::ensure_running()?;
    let url = crate::with_core(app.state::<crate::Shared>().inner().clone(), |c| {
        Ok(c.profile(c.session.snapshot().profile_id)?
            .details
            .connections
            .voice_studio_url)
    })
    .await?;
    app.state::<VoiceStartup>().start(&url, recheck)?;
    Ok(())
}

#[cfg(test)]
#[path = "voice_startup_tests.rs"]
mod tests;
