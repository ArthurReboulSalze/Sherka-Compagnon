//! A voice client for the server-owned agent. No local prompt, history,
//! memory extraction, model selection or tool execution is sent to Hermes.
use crate::{
    Error, Result,
    connections::Connections,
    providers::{ChatOutcome, Providers, failure, network},
};
use futures_util::StreamExt;
use reqwest::{Method, RequestBuilder, header::HeaderValue};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::mpsc;

pub fn profile_name(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    {
        return Err(failure("Le nom du profil Hermes est invalide."));
    }
    Ok(value.to_owned())
}

pub fn server_url(value: &str) -> Result<String> {
    let base = crate::connections::base_url(value)?;
    let url = url::Url::parse(&base).map_err(|_| failure("Adresse Hermes invalide."))?;
    let private = match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback() || ip.is_private(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback() || ip.is_unique_local(),
        _ => false,
    };
    if url.scheme() != "https" && !private || url.path().split('/').any(|s| s == "p") {
        return Err(failure(
            "Hermes exige HTTPS ou une adresse IP privée. Indique le serveur sans préfixe de profil.",
        ));
    }
    Ok(base)
}

pub fn profile_url(config: &Connections) -> Result<String> {
    let base = server_url(
        config
            .llm_api_url
            .as_deref()
            .ok_or_else(|| failure("Indique l’adresse du serveur Hermes."))?,
    )?;
    let profile = profile_name(config.hermes_profile.as_deref().unwrap_or("default"))?;
    Ok(if profile == "default" {
        base
    } else {
        format!("{base}/p/{profile}")
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct Approval {
    pub run_id: String,
    pub request_id: Option<String>,
    pub description: String,
    pub allow_once: bool,
}

pub enum AgentEvent {
    Session(String),
    Notice(&'static str),
    Approval(Option<Approval>),
}

#[derive(Clone)]
pub struct HermesRun {
    provider: Providers,
    id: Arc<Mutex<Option<String>>>,
}

impl HermesRun {
    pub fn new(provider: Providers) -> Self {
        Self {
            provider,
            id: Arc::new(Mutex::new(None)),
        }
    }

    /// Audio cancellation is immediate; remote interruption is cooperative.
    pub async fn stop(&self) -> Result<()> {
        let id = self.id.lock().map_err(|_| Error::StaleOperation)?.clone();
        if let Some(id) = id {
            let response = self
                .provider
                .hermes_request(Method::POST, &format!("/v1/runs/{id}/stop"))?
                .json(&json!({}))
                .timeout(Duration::from_secs(5))
                .send()
                .await
                .map_err(|e| network("Hermes", &e))?;
            if response.status().as_u16() != 404 {
                Providers::checked(response, "Hermes").await?;
            }
        }
        Ok(())
    }

    pub async fn chat(
        &self,
        session: &str,
        memory_scope: &str,
        user: &str,
        tokens: mpsc::Sender<String>,
        mut event: impl FnMut(AgentEvent) -> Result<()>,
    ) -> Result<ChatOutcome> {
        self.provider.ensure_hermes_session(session).await?;
        let response = self
            .provider
            .hermes_request(
                Method::POST,
                &format!("/api/sessions/{session}/chat/stream"),
            )?
            .header("X-Hermes-Session-Key", format!("sherka:{memory_scope}"))
            .json(&json!({"message": user}))
            .timeout(Duration::from_secs(3600))
            .send()
            .await
            .map_err(|e| network("Hermes", &e))?;
        let response = Providers::checked(response, "Hermes").await?;
        let mut stream = response.bytes_stream();
        let mut sse = AgentSse::default();
        let mut sent = String::new();
        let mut completed = false;
        let mut outcome = ChatOutcome::default();
        while let Some(chunk) = tokio::time::timeout(Duration::from_secs(60), stream.next())
            .await
            .map_err(|_| {
                failure("Hermes ne transmet plus de réponse. Vérifie la connexion au serveur.")
            })?
        {
            for (name, value) in sse.feed(&chunk.map_err(|e| network("Hermes", &e))?)? {
                if let Some(id) = value["run_id"].as_str() {
                    valid_id(id)?;
                    let mut current = self.id.lock().map_err(|_| Error::StaleOperation)?;
                    if current.as_deref().is_some_and(|old| old != id) {
                        return Err(failure("Flux Hermes incompatible."));
                    }
                    *current = Some(id.into());
                }
                match name.as_str() {
                    "assistant.delta" => {
                        if let Some(text) = value["delta"].as_str() {
                            if sent.len() + text.len() > 512_000 {
                                return Err(failure("Réponse Hermes trop volumineuse."));
                            }
                            sent.push_str(text);
                            tokens
                                .send(text.to_owned())
                                .await
                                .map_err(|_| Error::StaleOperation)?;
                        }
                    }
                    "assistant.completed" => {
                        if let Some(tip) = value["session_id"].as_str() {
                            valid_id(tip)?;
                            event(AgentEvent::Session(tip.into()))?;
                        }
                        if let Some(text) = value["content"].as_str().filter(|s| !s.is_empty()) {
                            // Some model backends only supply the completed answer. Others
                            // stream commentary plus the answer: never speak the answer twice.
                            let tail = if let Some(tail) = text.strip_prefix(&sent) {
                                tail.to_owned()
                            } else if sent.ends_with(text) {
                                String::new()
                            } else {
                                format!("{}{text}", if sent.is_empty() { "" } else { "\n\n" })
                            };
                            if !tail.is_empty() {
                                if sent.len() + tail.len() > 512_000 {
                                    return Err(failure("Réponse Hermes trop volumineuse."));
                                }
                                sent.push_str(&tail);
                                tokens.send(tail).await.map_err(|_| Error::StaleOperation)?;
                            }
                        }
                    }
                    "tool.started" => event(AgentEvent::Notice("Hermes utilise ses outils…"))?,
                    "tool.completed" | "tool.failed" => {
                        event(AgentEvent::Notice("Hermes prépare sa réponse…"))?
                    }
                    "approval.request" => {
                        let id = value["run_id"]
                            .as_str()
                            .ok_or_else(|| failure("Flux Hermes incompatible."))?;
                        let request_id = value["request_id"].as_str().map(str::to_owned);
                        if let Some(id) = &request_id {
                            valid_id(id)?;
                        }
                        let description = ["description", "command", "reason"]
                            .iter()
                            .filter_map(|key| value[*key].as_str())
                            .collect::<Vec<_>>()
                            .join("\n")
                            .chars()
                            .take(8000)
                            .collect();
                        let allow_once = value["choices"].as_array().is_some_and(|choices| {
                            choices.iter().any(|v| v == "once" || v["value"] == "once")
                        });
                        event(AgentEvent::Approval(Some(Approval {
                            run_id: id.into(),
                            request_id,
                            description,
                            allow_once,
                        })))?;
                    }
                    "approval.resolved" | "approval.cancelled" => {
                        event(AgentEvent::Approval(None))?
                    }
                    "run.completed" => {
                        completed = true;
                        outcome.prompt_tokens = value["usage"]["input_tokens"]
                            .as_u64()
                            .or(value["usage"]["prompt_tokens"].as_u64());
                        outcome.completion_tokens = value["usage"]["output_tokens"]
                            .as_u64()
                            .or(value["usage"]["completion_tokens"].as_u64());
                        event(AgentEvent::Approval(None))?;
                        break;
                    }
                    "run.cancelled" | "run.interrupted" => {
                        return Err(failure("Hermes a interrompu la réponse."));
                    }
                    "error" | "run.failed" => {
                        return Err(failure(
                            "Hermes n’a pas pu terminer la réponse. Consulte ses journaux.",
                        ));
                    }
                    // Reasoning, tool arguments/results and duplicate commentary never enter speech.
                    _ => {}
                }
            }
            if completed {
                break;
            }
        }
        if !completed {
            return Err(failure(
                "La connexion Hermes s’est fermée avant la fin de la réponse.",
            ));
        }
        Ok(outcome)
    }
}

fn valid_id(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 256
        || !value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b':'))
    {
        return Err(failure("Identifiant Hermes invalide."));
    }
    Ok(())
}

impl Providers {
    fn hermes_request(&self, method: Method, path: &str) -> Result<RequestBuilder> {
        if !self.connections.llm_provider.is_agent() {
            return Err(Error::StaleOperation);
        }
        let key = self
            .api_key
            .as_ref()
            .ok_or_else(|| failure("Enregistre la clé API du profil Hermes dans Réglages."))?;
        let mut auth = HeaderValue::from_str(&format!("Bearer {key}"))
            .map_err(|_| failure("La clé API est vide ou invalide."))?;
        auth.set_sensitive(true);
        Ok(self
            .http
            .request(method, format!("{}{path}", profile_url(&self.connections)?))
            .header("Authorization", auth)
            .timeout(Duration::from_secs(15)))
    }

    pub async fn hermes_probe(&self) -> Result<()> {
        let response = self
            .hermes_request(Method::GET, "/v1/capabilities")?
            .send()
            .await
            .map_err(|e| network("Hermes", &e))?;
        let value = Self::api_json(response, "Hermes", 256_000).await?;
        if value["platform"] != "hermes-agent"
            || value["endpoints"]["session_chat_stream"]["path"]
                != "/api/sessions/{session_id}/chat/stream"
            || value["features"]["run_stop"] != true
        {
            return Err(failure(
                "Cette version de Hermes ne propose pas les sessions vocales requises. Mets Hermes à jour.",
            ));
        }
        Ok(())
    }

    async fn ensure_hermes_session(&self, session: &str) -> Result<()> {
        valid_id(session)?;
        let response = self
            .hermes_request(Method::GET, &format!("/api/sessions/{session}"))?
            .send()
            .await
            .map_err(|e| network("Hermes", &e))?;
        if response.status().as_u16() != 404 {
            Self::checked(response, "Hermes").await?;
            return Ok(());
        }
        let response = self
            .hermes_request(Method::POST, "/api/sessions")?
            .json(&json!({"id":session,"source":"api_server"}))
            .send()
            .await
            .map_err(|e| network("Hermes", &e))?;
        if response.status().as_u16() != 409 {
            Self::checked(response, "Hermes").await?;
        }
        Ok(())
    }

    pub async fn hermes_approve(&self, approval: &Approval, allow: bool) -> Result<()> {
        valid_id(&approval.run_id)?;
        if allow && !approval.allow_once {
            return Err(Error::StaleOperation);
        }
        let mut body = json!({"choice":if allow { "once" } else { "deny" }});
        if let Some(id) = &approval.request_id {
            valid_id(id)?;
            body["request_id"] = json!(id);
        }
        let response = self
            .hermes_request(
                Method::POST,
                &format!("/v1/runs/{}/approval", approval.run_id),
            )?
            .json(&body)
            .send()
            .await
            .map_err(|e| network("Hermes", &e))?;
        Self::checked(response, "Hermes").await?;
        Ok(())
    }
}

#[derive(Default)]
struct AgentSse {
    pending: Vec<u8>,
    data: String,
    event: String,
}
impl AgentSse {
    fn feed(&mut self, bytes: &[u8]) -> Result<Vec<(String, Value)>> {
        self.pending.extend_from_slice(bytes);
        if self.pending.len() > 1_048_576 {
            return Err(failure("Événement SSE trop volumineux."));
        }
        let mut events = vec![];
        while let Some(end) = self.pending.iter().position(|b| *b == b'\n') {
            let raw: Vec<u8> = self.pending.drain(..=end).collect();
            let line = std::str::from_utf8(&raw)
                .map_err(|_| failure("Encodage SSE invalide."))?
                .trim_end_matches(['\r', '\n']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    let value = serde_json::from_str(&self.data)
                        .map_err(|_| failure("Flux Hermes incompatible."))?;
                    events.push((std::mem::take(&mut self.event), value));
                    self.data.clear();
                } else {
                    self.event.clear();
                }
            } else if let Some(value) = line.strip_prefix("event:") {
                self.event = value.trim().into();
            } else if let Some(value) = line.strip_prefix("data:") {
                if !self.data.is_empty() {
                    self.data.push('\n');
                }
                self.data.push_str(value.strip_prefix(' ').unwrap_or(value));
                if self.data.len() > 1_048_576 {
                    return Err(failure("Événement SSE trop volumineux."));
                }
            }
        }
        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_named_events_ignore_heartbeats() {
        let raw = ": keepalive\r\nevent: assistant.delta\r\ndata: {\"delta\":\"été\"}\r\n\r\n";
        let mut sse = AgentSse::default();
        let mut events = vec![];
        for byte in raw.as_bytes() {
            events.extend(sse.feed(&[*byte]).unwrap());
        }
        assert_eq!(
            events,
            vec![("assistant.delta".into(), json!({"delta":"été"}))]
        );
    }
}
