use serde_json::{Value, json};
use sherka_core::{
    catalogs::{CatalogClient, DiscoveryError},
    connections::{Connections, base_url},
};
use std::{
    collections::HashMap,
    io::{BufRead, BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

struct Server {
    url: String,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
}
impl Server {
    fn new(routes: HashMap<String, (u16, String)>) -> Self {
        let socket = TcpListener::bind("127.0.0.1:0").unwrap();
        socket.set_nonblocking(true).unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let worker = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match socket.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_secs(2)))
                            .unwrap();
                        // TCP can split a request at any byte. Closed/retried
                        // connections are normal and must not kill the fixture.
                        let mut buffer = [0; 4096];
                        let mut headers = Vec::new();
                        while headers.len() <= 8192
                            && !headers.windows(4).any(|part| part == b"\r\n\r\n")
                        {
                            match stream.read(&mut buffer) {
                                Ok(length) if length > 0 => {
                                    headers.extend_from_slice(&buffer[..length]);
                                }
                                _ => break,
                            }
                        }
                        if !headers.windows(4).any(|part| part == b"\r\n\r\n") {
                            continue;
                        }
                        let request = String::from_utf8_lossy(&headers);
                        assert!(request.starts_with("GET "));
                        let path = request.split_whitespace().nth(1).unwrap();
                        let (status, body) =
                            routes.get(path).cloned().unwrap_or((404, "{}".into()));
                        let _ = write!(
                            stream,
                            "HTTP/1.1 {status} Response\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                            body.len()
                        );
                    }
                    Err(_) => thread::sleep(Duration::from_millis(5)),
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
impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let result = self.worker.take().unwrap().join();
        if !thread::panicking() {
            result.expect("HTTP fixture worker failed");
        }
    }
}

fn routes() -> HashMap<String, (u16, String)> {
    let fixture: HashMap<String, Value> =
        serde_json::from_str(include_str!("../../../tests/fixtures/services.json")).unwrap();
    fixture
        .into_iter()
        .map(|(path, json)| (path, (200, json.to_string())))
        .collect()
}

#[test]
fn mock_http_accepts_fragmented_headers_and_canceled_connections() {
    let server = Server::new(HashMap::from([("/models".into(), (200, "{}".into()))]));
    let address = server.url.strip_prefix("http://").unwrap();
    drop(TcpStream::connect(address).unwrap());
    let mut stream = TcpStream::connect(address).unwrap();
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    for part in ["G", "ET /models HTTP/1.1\r\nHost: local", "host\r\n\r\n"] {
        stream.write_all(part.as_bytes()).unwrap();
        thread::sleep(Duration::from_millis(20));
    }
    let mut stream = BufReader::new(stream);
    let mut status = String::new();
    stream.read_line(&mut status).unwrap();
    assert!(status.starts_with("HTTP/1.1 200"));
    loop {
        let mut header = String::new();
        assert!(stream.read_line(&mut header).unwrap() > 0);
        if header == "\r\n" {
            break;
        }
    }
    let mut body = [0; 2];
    stream.read_exact(&mut body).unwrap();
    assert_eq!(&body, b"{}");
}

#[tokio::test]
async fn real_http_contracts_distinguish_models_engines_aliases_and_loaded_instances() {
    let server = Server::new(routes());
    let client = CatalogClient::new(&server.url).unwrap();
    let llm = client.language_models().await.unwrap();
    assert_eq!(llm.models.len(), 2);
    assert_eq!(llm.excluded_models, 1);
    assert_eq!(llm.models[0].id, "my-loaded-chat");
    assert_eq!(llm.models[0].loaded, Some(true));
    assert_eq!(llm.models[1].loaded, Some(false));
    assert_eq!(llm.models[1].quantization, None);
    assert!(llm.models[0].reasoning.is_none());
    assert_eq!(
        llm.models[1].reasoning.as_ref().unwrap().allowed_options,
        ["off", "low", "medium", "high"]
    );
    let mut settings = Connections {
        reasoning: Some("high".into()),
        ..Default::default()
    };
    assert!(settings.validate_reasoning(&llm.models[1]).is_ok());
    assert!(settings.validate_reasoning(&llm.models[0]).is_err());
    assert_eq!(settings.reasoning_effort(), Some("high"));
    settings.reasoning = Some("off".into());
    assert_eq!(settings.reasoning_effort(), Some("none"));
    settings.reasoning = Some("on".into());
    assert_eq!(settings.reasoning_effort(), Some("medium"));

    let voice = client.voices().await.unwrap();
    assert_eq!(
        voice
            .tts_engines
            .iter()
            .map(|e| e.readiness.as_str())
            .collect::<Vec<_>>(),
        ["ready", "missing_model", "missing_engine"]
    );
    assert_eq!(voice.voices.len(), 2);
    assert_ne!(voice.voices[0].voice_id, voice.voices[1].voice_id);
    assert_eq!(voice.voice_alias_count, 1);
    assert_eq!(voice.stt_models, ["faster-whisper"]);
}

#[tokio::test]
async fn fallback_requires_missing_route_and_never_hides_auth_or_malformed_responses() {
    let server = Server::new(HashMap::from([("/api/v0/models".into(), (200, json!({"data":[{"id":"legacy","type":"llm","state":"loaded","quantization":"Q4","max_context_length":8192}]}).to_string()))]));
    assert_eq!(
        CatalogClient::new(&server.url)
            .unwrap()
            .language_models()
            .await
            .unwrap()
            .source,
        "/api/v0/models"
    );
    for (status, body) in [(401, "{}"), (200, "<html>not a catalogue</html>")] {
        let server = Server::new(HashMap::from([(
            "/api/v1/models".into(),
            (status, body.into()),
        )]));
        let error = CatalogClient::new(&server.url)
            .unwrap()
            .language_models()
            .await
            .unwrap_err();
        assert!(matches!(
            (status, error),
            (401, DiscoveryError::Authentication) | (200, DiscoveryError::Format(_))
        ));
    }
}

#[tokio::test]
async fn oversized_responses_and_unknown_voice_protocol_are_rejected() {
    let server = Server::new(HashMap::from([(
        "/api/v1/models".into(),
        (200, "x".repeat(2 * 1024 * 1024 + 1)),
    )]));
    assert!(matches!(
        CatalogClient::new(&server.url)
            .unwrap()
            .language_models()
            .await,
        Err(DiscoveryError::TooLarge)
    ));
    let mut fixture = routes();
    fixture.insert("/.well-known/voicestudio-speech".into(), (200, json!({"protocol":"future.v9","service_version":"9","features":{"streaming_transcription":true,"batch_transcription":true}}).to_string()));
    let server = Server::new(fixture);
    assert!(matches!(
        CatalogClient::new(&server.url).unwrap().voices().await,
        Err(DiscoveryError::Format(_))
    ));
}

#[tokio::test]
async fn stopped_service_is_reported_without_retry_or_fallback() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    drop(listener);
    assert!(matches!(
        CatalogClient::new(&url).unwrap().language_models().await,
        Err(DiscoveryError::Unreachable)
    ));
}

#[test]
fn endpoints_cannot_contain_secrets_and_existing_profiles_receive_defaults() {
    assert_eq!(
        base_url(" http://localhost:1234/api/v1/ ").unwrap(),
        "http://localhost:1234"
    );
    for url in [
        "file:///private",
        "http://user:password@localhost:1234",
        "http://localhost:1234/?api_key=secret",
        "http://localhost/#secret",
    ] {
        assert!(base_url(url).is_err());
    }
    let legacy: sherka_core::model::ProfileDraft = serde_json::from_value(
        json!({"name":"Legacy","personality":"","instructions":"","accent":"amber"}),
    )
    .unwrap();
    assert_eq!(legacy.connections, Connections::default());
}
