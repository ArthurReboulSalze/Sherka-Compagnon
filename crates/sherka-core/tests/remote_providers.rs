use serde_json::{Value, json};
use sherka_core::{
    connections::{Connections, LlmProvider, api_base_url},
    conversation::{ChatMessage, budget},
    model::ProfileDraft,
    models::ModelManager,
    providers::Providers,
};
use std::{
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use tokio::sync::mpsc;

struct Fixture {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Fixture {
    fn new(
        handler: impl Fn(&str, &str, Value) -> (u16, &'static str, String) + Send + 'static,
    ) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                let Ok((mut stream, _)) = socket.accept() else {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                };
                // Windows can inherit the listener's nonblocking mode. Body
                // fragments may arrive after accept; wait up to the timeout.
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                let mut raw = Vec::new();
                let end;
                loop {
                    let mut bytes = [0; 4096];
                    let n = stream.read(&mut bytes).unwrap();
                    if n == 0 {
                        return;
                    }
                    raw.extend_from_slice(&bytes[..n]);
                    if let Some(i) = raw.windows(4).position(|p| p == b"\r\n\r\n") {
                        end = i + 4;
                        break;
                    }
                }
                let header = String::from_utf8(raw[..end].to_vec()).unwrap();
                let length = header
                    .lines()
                    .find_map(|l| {
                        l.to_ascii_lowercase()
                            .strip_prefix("content-length:")
                            .and_then(|s| s.trim().parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while raw.len() < end + length {
                    let mut bytes = [0; 4096];
                    let n = stream.read(&mut bytes).unwrap();
                    assert!(n > 0);
                    raw.extend_from_slice(&bytes[..n]);
                }
                let value = if length > 0 {
                    serde_json::from_slice(&raw[end..end + length]).unwrap()
                } else {
                    Value::Null
                };
                let path = header.split_whitespace().nth(1).unwrap();
                let (status, mime, body) = handler(path, &header.to_ascii_lowercase(), value);
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status} Response\r\nContent-Type: {mime}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                // Exercise SSE frames split inside multibyte UTF-8 sequences.
                for bytes in body.as_bytes().chunks(7) {
                    if stream.write_all(bytes).is_err() {
                        break;
                    }
                }
            }
        });
        Self {
            url,
            stop,
            worker: Some(worker),
        }
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let result = self.worker.take().unwrap().join();
        if !thread::panicking() {
            result.unwrap();
        }
    }
}
fn config(kind: LlmProvider, url: &str) -> Connections {
    Connections {
        llm_provider: kind,
        llm_api_url: Some(format!("{url}/base/v1")),
        llm_model: Some("test-model".into()),
        ..Default::default()
    }
}
fn message(role: &'static str, content: &str) -> ChatMessage {
    ChatMessage {
        role,
        content: content.into(),
    }
}

#[tokio::test]
async fn local_structured_tasks_disable_optional_thinking_retry_grammar_and_keep_context_bounds() {
    let seen = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = seen.clone();
    let fixture = Fixture::new(move |path, _, body| {
        if path == "/api/v1/models" {
            return (200,"application/json",json!({"models":[{"type":"llm","key":"test-model","display_name":"Fixture","capabilities":{"reasoning":{"allowed_options":["off","on"],"default":"on"}},"max_context_length":8192,"loaded_instances":[{"id":"test-model","config":{"context_length":8192}}]}]}).to_string());
        }
        assert_eq!(path, "/v1/chat/completions");
        let mut captured = captured.lock().unwrap();
        captured.push(body.clone());
        assert_eq!(body["reasoning_effort"], "none");
        assert_eq!(body["stream"], false);
        assert_eq!(body["messages"][0]["role"], "system");
        assert!(
            body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .skip(1)
                .all(|m| m["role"] != "system")
        );
        let input: usize = body["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| m["content"].as_str().unwrap().len() + 32)
            .sum();
        assert!(input + body["max_tokens"].as_u64().unwrap() as usize + 512 <= 8192);
        if captured.len() == 1 {
            assert!(body.get("response_format").is_some());
            return (400, "application/json", "{}".into());
        }
        assert!(body.get("response_format").is_none());
        (200,"application/json",json!({"choices":[{"finish_reason":"stop","message":{"reasoning_content":"private, never parse as the result","content":"<think>private</think>```json\n{\"include_companion\":false}\n```"}}]}).to_string())
    });
    let provider = Providers::new(Connections {
        lm_studio_url: fixture.url.clone(),
        llm_model: Some("test-model".into()),
        reasoning: Some("on".into()),
        ..Default::default()
    })
    .unwrap();
    let (messages, schema) = sherka_core::illustration::subject_request(
        "Kit",
        "Imagine a mountain landscape",
        "Snowy peaks.",
    );
    let result: sherka_core::illustration::Subject = provider
        .structured_budgeted(messages, schema, 8192, 128)
        .await
        .unwrap();
    assert!(!result.include_companion);
    assert_eq!(seen.lock().unwrap().len(), 2);
}
fn sse(value: Value) -> String {
    format!("data: {value}\r\n\r\n")
}
async fn chat(
    provider: &Providers,
) -> (
    sherka_core::Result<sherka_core::providers::ChatOutcome>,
    String,
) {
    let (tx, mut rx) = mpsc::channel(64);
    tokio::join!(
        provider.chat_budgeted(
            vec![message("system", "Persona"), message("user", "été")],
            tx,
            512
        ),
        async move {
            let mut text = String::new();
            while let Some(chunk) = rx.recv().await {
                text.push_str(&chunk);
            }
            text
        }
    )
}

#[test]
fn api_bases_preserve_paths_and_reject_credentials_or_public_plaintext() {
    assert_eq!(
        api_base_url("https://openrouter.ai/api/v1/").unwrap(),
        "https://openrouter.ai/api/v1"
    );
    assert_eq!(
        api_base_url("https://api.z.ai/api/paas/v4").unwrap(),
        "https://api.z.ai/api/paas/v4"
    );
    assert_eq!(
        LlmProvider::ZaiCoding.default_url(),
        Some("https://api.z.ai/api/coding/paas/v4")
    );
    assert_ne!(
        LlmProvider::Zai.default_url(),
        LlmProvider::ZaiCoding.default_url()
    );
    for url in [
        "http://example.com/v1",
        "https://user:password@localhost/v1",
        "https://example.com/v1?key=secret",
        "https://example.com/#secret",
    ] {
        assert!(api_base_url(url).is_err());
    }
    let old: Connections = serde_json::from_str(r#"{"llm_model":"local"}"#).unwrap();
    assert!(old.llm_provider.is_local());
    let value = serde_json::to_value(old).unwrap();
    assert!(value.get("llm_provider").is_none());
    assert!(value.get("llm_api_url").is_none());
}

#[tokio::test]
async fn openrouter_catalog_budget_reasoning_and_sse_keep_thoughts_out_of_voice() {
    let server = Fixture::new(|path, header, body| {
        assert!(header.contains("authorization: bearer fixture-token"));
        assert!(!header.contains("x-api-key:"));
        match path {
            "/base/v1/models" => (200,"application/json",json!({"data":[{"id":"test-model","name":"Flash","context_length":131072,"top_provider":{"max_completion_tokens":300},"supported_parameters":["structured_outputs"],"reasoning":{"mandatory":true,"supported_efforts":["high","low"],"default_effort":"high"}},{"id":"image-only","architecture":{"output_modalities":["image"]}}]}).to_string()),
            "/base/v1/chat/completions" => {
                assert_eq!(body["reasoning"],json!({"effort":"low","exclude":true})); assert_eq!(body["max_tokens"],300); assert_eq!(body["stream_options"]["include_usage"],true);
                (200,"text/event-stream", ": heartbeat\r\n\r\n".to_owned()+&sse(json!({"choices":[{"delta":{"reasoning":"PRIVATE THOUGHT"}}]}))+&sse(json!({"choices":[{"delta":{"content":"été fini"},"finish_reason":"stop"}]}))+&sse(json!({"usage":{"prompt_tokens":120,"completion_tokens":20,"completion_tokens_details":{"reasoning_tokens":12}},"choices":[]}))+"data: [DONE]\n\n")
            }
            _ => panic!("Unexpected endpoint {path}"),
        }
    });
    let mut c = config(LlmProvider::OpenRouter, &server.url);
    c.reasoning = Some("low".into());
    c.context_limit = Some(8192);
    let mut provider = Providers::with_api_key(c.clone(), Some("fixture-token".into())).unwrap();
    let model = provider.remote_model().await.unwrap();
    assert!(
        !model
            .reasoning
            .as_ref()
            .unwrap()
            .allowed_options
            .iter()
            .any(|s| s == "off")
    );
    c.validate_reasoning(&model).unwrap();
    let plan = budget(
        &ProfileDraft {
            connections: c,
            ..Default::default()
        },
        &model,
    )
    .unwrap();
    assert_eq!(plan.limit, 8192);
    assert_eq!(plan.response_tokens, 300);
    provider.use_api_model(&model);
    let (result, text) = chat(&provider).await;
    let usage = result.unwrap();
    assert_eq!(text, "été fini");
    assert_eq!(usage.reasoning_tokens, Some(12));
    assert_eq!(usage.prompt_tokens, Some(120));
    provider.connections.reasoning = Some("off".into());
    assert!(provider.connections.validate_reasoning(&model).is_err());
}

#[tokio::test]
async fn anthropic_messages_system_headers_adaptive_thinking_usage_and_structured_memory() {
    let posts = Arc::new(Mutex::new(0));
    let received = posts.clone();
    let server = Fixture::new(move |path, header, body| {
        assert!(header.contains("x-api-key: fixture-token"));
        assert!(header.contains("anthropic-version: 2023-06-01"));
        assert!(!header.contains("authorization:"));
        if path.starts_with("/base/v1/models") {
            return (200,"application/json",json!({"data":[{"id":"test-model","display_name":"Claude fixture","max_input_tokens":200000,"max_tokens":4096,"capabilities":{"thinking":{"supported":true,"types":{"adaptive":{"supported":true},"disabled":{"supported":true}}},"effort":{"low":{"supported":true}},"structured_outputs":{"supported":true}}}],"has_more":false}).to_string());
        }
        assert_eq!(path, "/base/v1/messages");
        assert!(body["system"].as_str().unwrap().contains("Persona"));
        assert!(
            body["messages"]
                .as_array()
                .unwrap()
                .iter()
                .all(|m| m["role"] != "system")
        );
        *received.lock().unwrap() += 1;
        if body["stream"] == false {
            assert_eq!(body["output_config"]["format"]["type"], "json_schema");
            assert!(body.get("response_format").is_none());
            return (200,"application/json",json!({"stop_reason":"end_turn","content":[{"type":"thinking","thinking":"hidden"},{"type":"text","text":"{\"ok\":true}"}]}).to_string());
        }
        assert_eq!(body["thinking"]["type"], "adaptive");
        assert_eq!(body["output_config"]["effort"], "low");
        assert!(body.get("stream_options").is_none());
        (
            200,
            "text/event-stream",
            sse(
                json!({"type":"message_start","message":{"usage":{"input_tokens":10,"cache_read_input_tokens":20,"output_tokens":1}}}),
            ) + &sse(
                json!({"type":"content_block_delta","delta":{"type":"thinking_delta","thinking":"PRIVATE THOUGHT"}}),
            ) + &sse(
                json!({"type":"content_block_delta","delta":{"type":"text_delta","text":"été fini"}}),
            ) + &sse(
                json!({"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":15}}),
            ) + &sse(json!({"type":"message_stop"})),
        )
    });
    let mut c = config(LlmProvider::Anthropic, &server.url);
    c.reasoning = Some("low".into());
    let mut provider = Providers::with_api_key(c, Some("fixture-token".into())).unwrap();
    let model = provider.remote_model().await.unwrap();
    provider.use_api_model(&model);
    let (result, text) = chat(&provider).await;
    let usage = result.unwrap();
    assert_eq!(text, "été fini");
    assert_eq!(usage.prompt_tokens, Some(30));
    assert_eq!(usage.completion_tokens, Some(15));
    let structured:Value=provider.structured(vec![message("system","Persona"),message("user","JSON")],json!({"type":"object","properties":{"ok":{"type":"boolean"}},"required":["ok"],"additionalProperties":false}),8192).await.unwrap();
    assert_eq!(structured["ok"], true);
    assert_eq!(*posts.lock().unwrap(), 2);
}

#[tokio::test]
async fn zai_fallback_context_and_json_mode_preserve_mandatory_thinking() {
    for kind in [LlmProvider::Zai, LlmProvider::ZaiCoding] {
        let server = Fixture::new(|path, _, body| {
            if path.ends_with("/models") {
                return (404, "application/json", "{}".into());
            }
            assert!(path.ends_with("/chat/completions"));
            assert_eq!(body["response_format"]["type"], "json_object");
            assert_eq!(body["thinking"]["type"], "enabled");
            assert_eq!(body["reasoning_effort"], "low");
            (
                200,
                "application/json",
                json!({"choices":[{"finish_reason":"stop","message":{"content":"{\"ok\":true}"}}]})
                    .to_string(),
            )
        });
        let mut c = config(kind, &server.url);
        c.llm_model = Some("glm-5.3-flash".into());
        let provider = Providers::with_api_key(c.clone(), Some("fixture-token".into())).unwrap();
        let model = provider.remote_model().await.unwrap();
        let default_budget = budget(
            &ProfileDraft {
                connections: c.clone(),
                ..Default::default()
            },
            &model,
        )
        .unwrap();
        assert_eq!(default_budget.limit, 64_000);
        assert_eq!(default_budget.response_tokens, 8192);
        c.context_limit = Some(8192);
        let mut provider = Providers::with_api_key(c, Some("fixture-token".into())).unwrap();
        let model = provider.remote_model().await.unwrap();
        provider.use_api_model(&model);
        let result: Value = provider
            .structured(
                vec![message("user", "JSON")],
                json!({"type":"object"}),
                8192,
            )
            .await
            .unwrap();
        assert_eq!(result["ok"], true);
    }
}

#[tokio::test]
async fn broken_streams_and_http_errors_do_not_echo_provider_bodies_or_keys() {
    for (status, mime, body) in [
        (
            401,
            "application/json",
            "{\"error\":\"fixture-token private prompt\"}".to_owned(),
        ),
        (
            200,
            "text/event-stream",
            sse(json!({"choices":[{"delta":{"content":"Partial"}}]})) + "data: [DONE]\n\n",
        ),
    ] {
        let server = Fixture::new(move |_, _, _| (status, mime, body.clone()));
        let provider = Providers::with_api_key(
            config(LlmProvider::CustomOpenAi, &server.url),
            Some("fixture-token".into()),
        )
        .unwrap();
        let (result, _) = chat(&provider).await;
        let error = result.unwrap_err().to_string();
        assert!(!error.contains("fixture-token"));
        assert!(!error.contains("private prompt"));
    }
}

#[tokio::test]
async fn quota_errors_are_distinguished_from_transient_rate_limits_without_echoing_messages() {
    for (code, credit) in [("1113", true), ("1302", false)] {
        let server = Fixture::new(move |_, _, _| {
            (
                429,
                "application/json",
                json!({"error":{"code":code,"message":"fixture-token private prompt"}}).to_string(),
            )
        });
        let provider = Providers::with_api_key(
            config(LlmProvider::Zai, &server.url),
            Some("fixture-token".into()),
        )
        .unwrap();
        let (result, _) = chat(&provider).await;
        let error = result.unwrap_err().to_string();
        assert_eq!(error.contains("crédit API"), credit);
        assert!(!error.contains("fixture-token"));
        assert!(!error.contains("private prompt"));
    }
}

#[tokio::test]
async fn local_to_cloud_unloads_once_and_cloud_shutdown_never_calls_load_or_unload() {
    let calls = Arc::new(Mutex::new(Vec::<String>::new()));
    let seen = calls.clone();
    let loaded = Arc::new(AtomicBool::new(true));
    let state = loaded.clone();
    let server = Fixture::new(move |path, _, body| {
        seen.lock().unwrap().push(path.to_owned());
        if path == "/api/v1/models" {
            return (200,"application/json",json!({"models":[{"type":"llm","key":"local","display_name":"Local","max_context_length":8192,"loaded_instances":if state.load(Ordering::Relaxed) {vec![json!({"id":"local","config":{"context_length":8192}})]} else {vec![]}}]}).to_string());
        }
        if path == "/api/v1/models/unload" {
            assert_eq!(body["instance_id"], "local");
            state.store(false, Ordering::Relaxed);
            return (200, "application/json", "{}".into());
        }
        if path == "/base/v1/models" {
            return (
                200,
                "application/json",
                json!({"data":[{"id":"test-model","context_length":8192}]}).to_string(),
            );
        }
        panic!("Unexpected model mutation {path}");
    });
    let local = Connections {
        lm_studio_url: server.url.clone(),
        llm_model: Some("local".into()),
        ..Default::default()
    };
    let mut cloud = config(LlmProvider::CustomOpenAi, &server.url);
    cloud.lm_studio_url = server.url.clone();
    let manager = ModelManager::default();
    manager.activate(&local, None, || true).await.unwrap();
    manager
        .activate(&cloud, Some(&local), || true)
        .await
        .unwrap();
    manager.shutdown(&cloud).await.unwrap();
    assert!(!loaded.load(Ordering::Relaxed));
    assert_eq!(
        calls
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.ends_with("/unload"))
            .count(),
        1
    );
    assert!(!calls.lock().unwrap().iter().any(|p| p.ends_with("/load")));
}

#[cfg(windows)]
#[test]
fn protected_keys_are_scoped_by_provider_and_exact_endpoint_and_not_exported() {
    let url = format!("https://{}.invalid/v1", uuid::Uuid::new_v4());
    let c = config(LlmProvider::CustomOpenAi, &url);
    let mut other = c.clone();
    other.llm_provider = LlmProvider::CustomAnthropic;
    sherka_core::credentials::set(&c, Some("fixture-token")).unwrap();
    let result = std::panic::catch_unwind(|| {
        assert_eq!(
            sherka_core::credentials::load(&c).unwrap().as_deref(),
            Some("fixture-token")
        );
        assert!(sherka_core::credentials::load(&other).unwrap().is_none());
        assert!(!serde_json::to_string(&c).unwrap().contains("fixture-token"));
    });
    sherka_core::credentials::set(&c, None).unwrap();
    result.unwrap();
    assert!(sherka_core::credentials::load(&c).unwrap().is_none());
}
