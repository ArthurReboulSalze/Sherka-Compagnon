use futures_util::{SinkExt, StreamExt};
use sherka_core::{
    connections::Connections,
    providers::{Providers, wav},
};
use std::{
    io::{Read, Write},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::Message;

#[tokio::test]
async fn streaming_finals_are_display_only_until_local_end_and_summary_is_not_duplicated() {
    for summary in ["Oui. Encore.", "Encore."] {
        let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", socket.local_addr().unwrap());
        let server = tokio::spawn(async move {
            let (stream, _) = socket.accept().await.unwrap();
            let mut ws = tokio_tungstenite::accept_async(stream).await.unwrap();
            ws.send(Message::Text(
                r#"{"type":"session.started","protocol":"voicestudio.speech.v1"}"#.into(),
            ))
            .await
            .unwrap();
            let mut frames = 0;
            while let Some(Ok(event)) = ws.next().await {
                match event {
                    Message::Binary(pcm) => {
                        assert!(!pcm.is_empty());
                        frames += 1;
                        if frames == 1 {
                            ws.send(Message::Text(
                                r#"{"type":"final","final_kind":"utterance","text":"Oui."}"#.into(),
                            ))
                            .await
                            .unwrap();
                        } else {
                            ws.send(Message::Text(
                                r#"{"type":"final","final_kind":"utterance","text":"Encore."}"#
                                    .into(),
                            ))
                            .await
                            .unwrap();
                        }
                    }
                    Message::Text(s) if s.contains("input_audio.end") => {
                        ws.send(Message::Text(serde_json::json!({"type":"final","final_kind":"summary","engine":"sherpa-onnx-asr","text":summary}).to_string().into())).await.unwrap();
                        break;
                    }
                    _ => {}
                }
            }
            assert_eq!(frames, 2);
        });
        let provider = Providers::new(Connections {
            voice_studio_url: url,
            stt_model: Some("sherpa-parakeet-tdt-v3".into()),
            ..Default::default()
        })
        .unwrap();
        let (tx, rx) = mpsc::channel(16);
        let partials = Arc::new(AtomicUsize::new(0));
        let count = partials.clone();
        let task = tokio::spawn(async move {
            provider
                .transcribe(rx, move |_, notice| {
                    assert!(!notice);
                    count.fetch_add(1, Ordering::Relaxed);
                })
                .await
        });
        tx.send(vec![1000; 160]).await.unwrap();
        tokio::time::timeout(Duration::from_secs(3), async {
            while partials.load(Ordering::Relaxed) == 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .unwrap();
        assert!(!task.is_finished());
        tx.send(vec![1000; 160]).await.unwrap();
        drop(tx);
        assert_eq!(task.await.unwrap().unwrap(), "Oui. Encore.");
        server.await.unwrap();
    }
}

#[tokio::test]
async fn missing_websocket_falls_back_to_one_http_upload_with_original_pcm() {
    let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", socket.local_addr().unwrap());
    let samples: Vec<i16> = (0..4800).map(|n| (n % 300) as i16).collect();
    let expected = wav(&samples);
    let server = std::thread::spawn(move || {
        let mut posts = 0;
        for _ in 0..3 {
            let (mut stream, _) = socket.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut request = Vec::new();
            let end;
            loop {
                let mut bytes = [0; 4096];
                let n = stream.read(&mut bytes).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&bytes[..n]);
                if let Some(i) = request.windows(4).position(|w| w == b"\r\n\r\n") {
                    end = i + 4;
                    break;
                }
            }
            let header = String::from_utf8_lossy(&request[..end]).to_string();
            let length = header
                .lines()
                .find_map(|l| {
                    l.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .and_then(|n| n.trim().parse::<usize>().ok())
                })
                .unwrap_or(0);
            while request.len() < end + length {
                let mut bytes = [0; 8192];
                let n = stream.read(&mut bytes).unwrap();
                assert!(n > 0);
                request.extend_from_slice(&bytes[..n]);
            }
            if header.starts_with("POST /v1/audio/transcriptions ") {
                posts += 1;
                assert!(
                    request[end..]
                        .windows(expected.len())
                        .any(|w| w == expected)
                );
                let body = r#"{"text":"Oui."}"#;
                write!(stream,"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
            } else {
                write!(
                    stream,
                    "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .unwrap();
            }
        }
        assert_eq!(posts, 1);
    });
    let provider = Providers::new(Connections {
        voice_studio_url: url,
        stt_model: Some("faster-whisper".into()),
        ..Default::default()
    })
    .unwrap();
    let (tx, rx) = mpsc::channel(32);
    for frame in samples.chunks(160) {
        tx.send(frame.to_vec()).await.unwrap();
    }
    drop(tx);
    let mut fallback = false;
    assert_eq!(
        provider
            .transcribe(rx, |_, notice| {
                fallback |= notice;
            })
            .await
            .unwrap(),
        "Oui."
    );
    assert!(fallback);
    server.join().unwrap();
}
