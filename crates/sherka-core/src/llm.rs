//! Remote conversation adapters. VoiceStudio never receives provider credentials.
use crate::{
    Error, Result,
    catalogs::{ApiModelCapabilities, LanguageCatalog, LanguageModel, ReasoningCapabilities},
    connections::LlmProvider,
    conversation::ChatMessage,
    providers::{ChatOutcome, Providers, Sse, failure, network},
};
use futures_util::StreamExt;
use reqwest::{RequestBuilder, Response, header::HeaderValue};
use serde_json::{Value, json};
use std::time::Duration;
use tokio::sync::mpsc;

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}
fn number(value: &Value) -> Option<u64> {
    value.as_u64().filter(|n| *n >= 1024)
}

impl Providers {
    fn api_request(&self, path: &str, post: bool) -> Result<RequestBuilder> {
        let base = self.connections.api_base()?;
        let request = if post {
            self.http.post(format!("{base}{path}"))
        } else {
            self.http.get(format!("{base}{path}"))
        };
        let mut request = request.header(
            "Accept",
            if post {
                "text/event-stream, application/json"
            } else {
                "application/json"
            },
        );
        if self.connections.llm_provider.is_anthropic() {
            request = request.header("anthropic-version", "2023-06-01");
        }
        if let Some(key) = &self.api_key {
            let mut value =
                HeaderValue::from_str(&if self.connections.llm_provider.is_anthropic() {
                    key.clone()
                } else {
                    format!("Bearer {key}")
                })
                .map_err(|_| failure("La clé API est vide ou invalide."))?;
            value.set_sensitive(true);
            request = request.header(
                if self.connections.llm_provider.is_anthropic() {
                    "x-api-key"
                } else {
                    "Authorization"
                },
                value,
            );
        } else if !matches!(
            self.connections.llm_provider,
            LlmProvider::CustomOpenAi | LlmProvider::CustomAnthropic
        ) {
            return Err(failure(
                "Enregistre la clé API de ce fournisseur dans Réglages.",
            ));
        }
        Ok(request)
    }

    pub(crate) async fn api_json(
        response: Response,
        service: &str,
        maximum: usize,
    ) -> Result<Value> {
        let mut response = Self::checked(response, service).await?;
        if response
            .content_length()
            .is_some_and(|n| n > maximum as u64)
        {
            return Err(failure(
                "La réponse du fournisseur dépasse la limite de sécurité.",
            ));
        }
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.map_err(|e| network(service, &e))? {
            if bytes.len() + chunk.len() > maximum {
                return Err(failure(
                    "La réponse du fournisseur dépasse la limite de sécurité.",
                ));
            }
            bytes.extend_from_slice(&chunk);
        }
        // Parse errors must not include arbitrary returned contents or credentials.
        serde_json::from_slice(&bytes)
            .map_err(|_| failure("Le fournisseur a renvoyé un document JSON invalide."))
    }

    pub(crate) async fn api_catalog(&self) -> Result<LanguageCatalog> {
        let kind = self.connections.llm_provider;
        let mut models = Vec::new();
        let mut excluded_models = 0;
        let mut after: Option<String> = None;
        for _ in 0..10 {
            let mut request = self
                .api_request("/models", false)?
                .timeout(Duration::from_secs(15));
            if kind.is_anthropic() {
                request = request.query(&[("limit", "100")]);
                if let Some(id) = &after {
                    request = request.query(&[("after_id", id)]);
                }
            }
            let response = request
                .send()
                .await
                .map_err(|e| network(kind.label(), &e))?;
            if matches!(response.status().as_u16(), 404 | 405) {
                // Some compatible services (including Z.ai endpoints) do not
                // expose a catalog. A manual model + context is an explicit fallback.
                return Ok(LanguageCatalog {
                    source: "/models (manual)".into(),
                    models: vec![],
                    excluded_models: 0,
                });
            }
            let value = Self::api_json(response, kind.label(), 8 * 1024 * 1024).await?;
            let data = value["data"]
                .as_array()
                .ok_or_else(|| failure("Le catalogue du fournisseur est incompatible."))?;
            if data.len() + models.len() > 10_000 {
                return Err(failure("Le catalogue du fournisseur est trop volumineux."));
            }
            for item in data {
                let Some(id) = item["id"].as_str().filter(|s| {
                    !s.is_empty() && s.len() <= 512 && !s.chars().any(char::is_control)
                }) else {
                    continue;
                };
                if item["lifecycle"] == "retired"
                    || item["architecture"]["output_modalities"]
                        .as_array()
                        .is_some_and(|m| !m.iter().any(|v| v == "text"))
                {
                    excluded_models += 1;
                    continue;
                }
                let parameters = strings(&item["supported_parameters"]);
                let mut options = Vec::new();
                let adaptive =
                    item["capabilities"]["thinking"]["types"]["adaptive"]["supported"] == true;
                let mut default = None;
                if kind == LlmProvider::OpenRouter {
                    if let Some(reasoning) = item.get("reasoning").filter(|v| v.is_object()) {
                        if reasoning["mandatory"] != true {
                            options.push("off".into());
                        }
                        options.push("on".into());
                        options.extend(
                            strings(&reasoning["supported_efforts"])
                                .into_iter()
                                .filter(|s| {
                                    ["minimal", "low", "medium", "high", "xhigh", "max"]
                                        .contains(&s.as_str())
                                }),
                        );
                        default = reasoning["default_effort"]
                            .as_str()
                            .map(|s| if s == "none" { "off" } else { s }.to_owned());
                    }
                } else if kind.is_anthropic()
                    && item["capabilities"]["thinking"]["supported"] == true
                {
                    if item["capabilities"]["thinking"]["types"]["disabled"]["supported"] == true {
                        options.push("off".into());
                    }
                    if adaptive
                        || item["capabilities"]["thinking"]["types"]["enabled"]["supported"] == true
                    {
                        options.push("on".into());
                    }
                    if adaptive {
                        for effort in ["low", "medium", "high", "xhigh", "max"] {
                            if item["capabilities"]["effort"][effort]["supported"] == true {
                                options.push(effort.into());
                            }
                        }
                    }
                }
                let max_context = number(&item["context_length"])
                    .or_else(|| number(&item["max_input_tokens"]))
                    .or_else(|| number(&item["max_context_length"]));
                let max_output = item["top_provider"]["max_completion_tokens"]
                    .as_u64()
                    .or_else(|| item["max_tokens"].as_u64())
                    .and_then(|n| u32::try_from(n).ok())
                    .filter(|n| *n > 0);
                if !models.iter().any(|m: &LanguageModel| m.id == id) {
                    models.push(LanguageModel {
                        vision: item["capabilities"]["vision"]
                            .as_bool()
                            .or_else(|| item["capabilities"]["vision"]["supported"].as_bool())
                            .or_else(|| {
                                item["architecture"]["input_modalities"]
                                    .as_array()
                                    .map(|v| v.iter().any(|v| v == "image"))
                            }),
                        id: id.into(),
                        key: id.into(),
                        label: item["name"]
                            .as_str()
                            .or_else(|| item["display_name"].as_str())
                            .unwrap_or(id)
                            .into(),
                        loaded: None,
                        quantization: None,
                        max_context_length: max_context,
                        loaded_context_length: None,
                        reasoning: (!options.is_empty()).then_some(ReasoningCapabilities {
                            allowed_options: options,
                            default,
                        }),
                        api: Some(ApiModelCapabilities {
                            max_output_tokens: max_output,
                            structured_output: parameters.iter().any(|s| s == "structured_outputs")
                                || item["capabilities"]["structured_outputs"]["supported"] == true,
                            adaptive_thinking: adaptive,
                            parameters,
                        }),
                    });
                }
            }
            if value["has_more"] != true {
                break;
            }
            let last = value["last_id"]
                .as_str()
                .ok_or_else(|| failure("Le catalogue du fournisseur est incompatible."))?
                .to_owned();
            if after.as_ref() == Some(&last) {
                return Err(failure("Le catalogue du fournisseur est incompatible."));
            }
            after = Some(last);
        }
        Ok(LanguageCatalog {
            source: "/models".into(),
            models,
            excluded_models,
        })
    }

    pub async fn remote_model(&self) -> Result<LanguageModel> {
        let id = self
            .connections
            .llm_model
            .as_deref()
            .ok_or_else(|| failure("Choisis un modèle de conversation dans Réglages."))?;
        let catalog = self.api_catalog().await?;
        let mut model = if let Some(model) = catalog.models.iter().find(|model| model.id == id) {
            model.clone()
        } else if catalog.models.is_empty() {
            LanguageModel {
                vision: None,
                id: id.into(),
                key: id.into(),
                label: id.into(),
                loaded: None,
                quantization: None,
                max_context_length: None,
                loaded_context_length: None,
                reasoning: None,
                api: Some(ApiModelCapabilities::default()),
            }
        } else {
            return Err(failure(
                "Le modèle choisi n’est plus disponible chez ce fournisseur.",
            ));
        };
        // Z.ai does not publish a model discovery contract: allow documented
        // thinking controls for its explicit GLM family, never invent context.
        if self.connections.llm_provider.is_zai() {
            let lower = id.to_ascii_lowercase();
            let choices: &[&str] = if lower.starts_with("glm-5.3") {
                &["on", "low", "high", "max"]
            } else if lower.starts_with("glm-5.2") {
                &["off", "on", "low", "medium", "high", "xhigh", "max"]
            } else if ["glm-5", "glm-4.6", "glm-4.7"]
                .iter()
                .any(|s| lower.starts_with(s))
            {
                &["off", "on"]
            } else {
                &[]
            };
            if !choices.is_empty() {
                model.reasoning = Some(ReasoningCapabilities {
                    allowed_options: choices.iter().map(|s| (*s).into()).collect(),
                    default: None,
                });
            }
        }
        if model.reasoning.is_none()
            && matches!(
                self.connections.llm_provider,
                LlmProvider::OpenAi | LlmProvider::CustomOpenAi | LlmProvider::CustomAnthropic
            )
        {
            // Compatible catalogs often omit capabilities. These are explicit
            // overrides, validated by the provider; never sent by default.
            let choices: &[&str] = if self.connections.llm_provider.is_anthropic() {
                &["off", "on"]
            } else {
                &[
                    "off", "on", "minimal", "low", "medium", "high", "xhigh", "max",
                ]
            };
            model.reasoning = Some(ReasoningCapabilities {
                allowed_options: choices.iter().map(|s| (*s).into()).collect(),
                default: None,
            });
        }
        Ok(model)
    }
    pub fn use_api_model(&mut self, model: &LanguageModel) {
        self.api_model = Some(model.clone());
    }

    fn api_body(
        &self,
        messages: Vec<ChatMessage>,
        max_tokens: u32,
        stream: bool,
        structured: bool,
    ) -> Result<Value> {
        let kind = self.connections.llm_provider;
        let max_tokens = max_tokens.min(
            self.api_model
                .as_ref()
                .and_then(|m| m.api.as_ref())
                .and_then(|m| m.max_output_tokens)
                .unwrap_or(u32::MAX),
        );
        let model = self
            .connections
            .llm_model
            .as_deref()
            .ok_or_else(|| failure("Choisis un modèle de conversation dans Réglages."))?;
        let mut body = if kind.is_anthropic() {
            let system = messages
                .iter()
                .filter(|m| m.role == "system")
                .map(|m| m.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");
            let mut dialogue: Vec<Value> = Vec::new();
            for m in messages.into_iter().filter(|m| m.role != "system") {
                if let Some(last) = dialogue.last_mut().filter(|v| v["role"] == m.role) {
                    let content = format!(
                        "{}\n\n{}",
                        last["content"].as_str().unwrap_or(""),
                        m.content
                    );
                    last["content"] = json!(content);
                } else {
                    dialogue.push(json!({"role": m.role, "content": m.content}));
                }
            }
            json!({"model": model, "system": system, "messages": dialogue, "stream": stream, "max_tokens": max_tokens})
        } else {
            let mut value = json!({"model": model, "messages": messages, "stream": stream});
            value[if kind == LlmProvider::OpenAi {
                "max_completion_tokens"
            } else {
                "max_tokens"
            }] = json!(max_tokens);
            if stream {
                value["stream_options"] = json!({"include_usage": true});
            }
            value
        };
        // Do not force temperature: current thinking models may reject it.
        let choice = self.connections.reasoning.as_deref();
        if let Some(choice) = choice.filter(|_| !structured) {
            match kind {
                LlmProvider::OpenRouter => {
                    body["reasoning"] = match choice {
                        "off" => json!({"enabled":false,"exclude":true}),
                        "on" => json!({"enabled":true,"exclude":true}),
                        effort => json!({"effort":effort,"exclude":true}),
                    };
                }
                LlmProvider::Zai | LlmProvider::ZaiCoding => {
                    body["thinking"] = json!({"type":if choice == "off" { "disabled" } else { "enabled" }, "clear_thinking":true});
                    if !["off", "on"].contains(&choice) {
                        body["reasoning_effort"] = json!(choice);
                    }
                }
                _ if kind.is_anthropic() => {
                    if choice == "off" {
                        body["thinking"] = json!({"type":"disabled"});
                    } else if self
                        .api_model
                        .as_ref()
                        .and_then(|m| m.api.as_ref())
                        .is_some_and(|m| m.adaptive_thinking)
                    {
                        body["thinking"] = json!({"type":"adaptive"});
                        if choice != "on" {
                            body["output_config"] = json!({"effort":choice});
                        }
                    } else {
                        if max_tokens <= 1024 {
                            return Err(failure(
                                "La pensée Anthropic nécessite plus de 1 024 tokens de réponse. Augmente la réserve dans Réglages.",
                            ));
                        }
                        body["thinking"] = json!({"type":"enabled", "budget_tokens":(max_tokens / 2).max(1024).min(max_tokens - 1)});
                    }
                }
                _ => {
                    body["reasoning_effort"] = json!(self.connections.reasoning_effort());
                }
            }
        }
        if structured && self.connections.llm_provider.is_zai() {
            // GLM 5.3 requires thinking. Keep its smallest supported effort;
            // older models can disable thinking for bounded JSON operations.
            let mandatory = model.to_ascii_lowercase().starts_with("glm-5.3");
            body["thinking"] = json!({"type":if mandatory { "enabled" } else { "disabled" },"clear_thinking":true});
            if mandatory {
                body["reasoning_effort"] = json!("low");
            }
        } else if structured
            && kind == LlmProvider::OpenRouter
            && let Some(reasoning) = self.api_model.as_ref().and_then(|m| m.reasoning.as_ref())
        {
            body["reasoning"] = if reasoning.allowed_options.iter().any(|s| s == "off") {
                json!({"enabled":false,"exclude":true})
            } else if let Some(effort) = ["minimal", "low", "medium", "high", "xhigh", "max"]
                .into_iter()
                .find(|s| reasoning.allowed_options.iter().any(|v| v == s))
            {
                json!({"effort":effort,"exclude":true})
            } else {
                json!({"enabled":true,"exclude":true})
            };
        }
        Ok(body)
    }

    pub(crate) async fn api_chat(
        &self,
        messages: Vec<ChatMessage>,
        tokens: mpsc::Sender<String>,
        max_tokens: u32,
    ) -> Result<ChatOutcome> {
        let kind = self.connections.llm_provider;
        let mut body = self.api_body(messages, max_tokens, true, false)?;
        crate::attachments::inject(&mut body, &self.images, kind.is_anthropic())?;
        let response = self
            .api_request(
                if kind.is_anthropic() {
                    "/messages"
                } else {
                    "/chat/completions"
                },
                true,
            )?
            .json(&body)
            .send()
            .await
            .map_err(|e| network(kind.label(), &e))?;
        let response = Self::checked(response, kind.label()).await?;
        if !response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.starts_with("text/event-stream"))
        {
            return Err(failure(
                "Le fournisseur n’a pas renvoyé un flux de conversation SSE.",
            ));
        }
        let mut stream = response.bytes_stream();
        let mut decoder = Sse::default();
        let mut outcome = ChatOutcome::default();
        let mut total = 0;
        let mut finished = false;
        while let Some(bytes) = stream.next().await {
            for event in decoder.feed(&bytes.map_err(|e| network(kind.label(), &e))?)? {
                if event.trim() == "[DONE]" {
                    return complete(outcome, total, finished);
                }
                let value: Value = serde_json::from_str(&event).map_err(|_| {
                    failure("Le flux du fournisseur contient un événement invalide.")
                })?;
                if value.get("error").is_some() || value["type"] == "error" {
                    return Err(failure(
                        "Le fournisseur a signalé une erreur pendant la réponse.",
                    ));
                }
                let text = if kind.is_anthropic() {
                    match value["type"].as_str() {
                        Some("message_start") => {
                            let usage = &value["message"]["usage"];
                            outcome.prompt_tokens = usage["input_tokens"].as_u64().map(|n| {
                                n + usage["cache_read_input_tokens"].as_u64().unwrap_or(0)
                                    + usage["cache_creation_input_tokens"].as_u64().unwrap_or(0)
                            });
                            outcome.completion_tokens = usage["output_tokens"].as_u64();
                            None
                        }
                        Some("content_block_delta") if value["delta"]["type"] == "text_delta" => {
                            value["delta"]["text"].as_str()
                        }
                        Some("content_block_start") if value["content_block"]["type"] == "text" => {
                            value["content_block"]["text"].as_str()
                        }
                        Some("message_delta") => {
                            if let Some(n) = value["usage"]["output_tokens"].as_u64() {
                                outcome.completion_tokens = Some(n);
                            }
                            if let Some(reason) = value["delta"]["stop_reason"].as_str() {
                                finish(reason, true, &mut outcome)?;
                                finished = true;
                            }
                            None
                        }
                        Some("message_stop") => return complete(outcome, total, finished),
                        _ => None,
                    }
                } else {
                    if let Some(usage) = value.get("usage").filter(|v| v.is_object()) {
                        outcome.prompt_tokens = usage["prompt_tokens"].as_u64();
                        outcome.completion_tokens = usage["completion_tokens"].as_u64();
                        outcome.reasoning_tokens =
                            usage["completion_tokens_details"]["reasoning_tokens"].as_u64();
                    }
                    if let Some(reason) = value["choices"][0]["finish_reason"].as_str() {
                        finish(reason, false, &mut outcome)?;
                        finished = true;
                    }
                    value["choices"][0]["delta"]["content"].as_str()
                };
                if let Some(text) = text.filter(|s| !s.is_empty()) {
                    total += text.len();
                    if total > 32_000 {
                        return Err(failure(
                            "La réponse dépasse la limite de sécurité de Sherka.",
                        ));
                    }
                    tokens
                        .send(text.into())
                        .await
                        .map_err(|_| Error::StaleOperation)?;
                }
            }
        }
        complete(outcome, total, finished)
    }

    pub(crate) async fn api_structured<T: serde::de::DeserializeOwned>(
        &self,
        mut messages: Vec<ChatMessage>,
        schema: Value,
        max_tokens: u32,
        timeout: Duration,
        limit: u64,
    ) -> Result<T> {
        let kind = self.connections.llm_provider;
        // Providers without constrained decoding still get the schema; final
        // JSON and semantic memory guards remain mandatory before any write.
        crate::structured::request(&mut messages, &schema);
        let max_tokens = max_tokens.min(
            self.api_model
                .as_ref()
                .and_then(|m| m.api.as_ref())
                .and_then(|m| m.max_output_tokens)
                .unwrap_or(u32::MAX),
        );
        if crate::conversation::estimate(&messages) as u64 + u64::from(max_tokens) + 512 > limit {
            return Err(failure(
                "La demande structurée dépasse le budget du modèle ; aucune mémoire n’a été modifiée.",
            ));
        }
        let mut body = self.api_body(messages, max_tokens, false, true)?;
        let constrained = self
            .api_model
            .as_ref()
            .and_then(|m| m.api.as_ref())
            .is_some_and(|m| m.structured_output);
        if kind.is_anthropic() && constrained {
            body["output_config"] = json!({"format":{"type":"json_schema","schema":schema}});
        } else if !kind.is_anthropic() {
            if constrained {
                body["response_format"] = json!({"type":"json_schema","json_schema":{"name":"sherka_data","strict":true,"schema":schema}});
            } else if kind.is_zai() {
                body["response_format"] = json!({"type":"json_object"});
            }
        }
        let response = self
            .api_request(
                if kind.is_anthropic() {
                    "/messages"
                } else {
                    "/chat/completions"
                },
                true,
            )?
            .timeout(timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| network(kind.label(), &e))?;
        let value = Self::api_json(response, kind.label(), 64_000).await?;
        let content = if kind.is_anthropic() {
            if !["end_turn", "stop_sequence"].contains(&value["stop_reason"].as_str().unwrap_or(""))
            {
                return Err(failure(
                    "La proposition du fournisseur est incomplète ; aucune mémoire n’a été modifiée.",
                ));
            }
            value["content"]
                .as_array()
                .ok_or_else(|| failure("La proposition du fournisseur est invalide."))?
                .iter()
                .filter(|v| v["type"] == "text")
                .filter_map(|v| v["text"].as_str())
                .collect::<String>()
        } else {
            if value["choices"][0]["finish_reason"] != "stop" {
                return Err(failure(
                    "La proposition du fournisseur est incomplète ; aucune mémoire n’a été modifiée.",
                ));
            }
            value["choices"][0]["message"]["content"]
                .as_str()
                .ok_or_else(|| failure("La proposition du fournisseur est invalide."))?
                .to_owned()
        };
        crate::structured::parse(&content)
    }
}

fn finish(reason: &str, anthropic: bool, outcome: &mut ChatOutcome) -> Result<()> {
    outcome.length_limited = reason == if anthropic { "max_tokens" } else { "length" };
    if outcome.length_limited
        || if anthropic {
            ["end_turn", "stop_sequence"].contains(&reason)
        } else {
            reason == "stop"
        }
    {
        Ok(())
    } else {
        Err(failure(
            "Le fournisseur a arrêté la réponse sans texte exploitable.",
        ))
    }
}
fn complete(outcome: ChatOutcome, total: usize, finished: bool) -> Result<ChatOutcome> {
    if !finished {
        return Err(failure(
            "Le fournisseur a fermé la connexion avant la fin de sa réponse.",
        ));
    }
    if total == 0 {
        return Err(failure(if outcome.length_limited {
            "Le modèle a épuisé sa réserve de réponse pendant la réflexion. Réduis la pensée ou augmente les tokens réservés à la réponse dans Réglages."
        } else {
            "Le fournisseur a renvoyé une réponse vide."
        }));
    }
    Ok(outcome)
}
