//! Optional search APIs. Keys go only to the fixed provider endpoint.
use crate::{Error, Result, web::Source};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Provider {
    #[default]
    DuckDuckGo,
    Brave,
    Tavily,
    Searxng,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub provider: Provider,
    pub searxng_url: String,
    pub max_results: u8,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            provider: Provider::DuckDuckGo,
            searxng_url: "http://127.0.0.1:8080".into(),
            max_results: 6,
        }
    }
}
fn invalid() -> Error {
    Error::Validation("La configuration du moteur de recherche est invalide.".into())
}
fn network_failure(error: &reqwest::Error) -> Error {
    use std::error::Error as _;
    let mut cause = error.source();
    while let Some(current) = cause {
        if current
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| e.raw_os_error() == Some(10013))
        {
            return Error::Validation("Windows bloque la connexion de Sherka au moteur de recherche (10013). Autorise cet exécutable dans ton pare-feu.".into());
        }
        cause = current.source();
    }
    Error::Validation(
        if error.is_timeout() {
            "Le moteur de recherche met trop de temps à répondre. Réessaie."
        } else {
            "Le moteur de recherche ne répond pas. Vérifie ta connexion Internet."
        }
        .into(),
    )
}
impl Config {
    pub fn validated(mut self) -> Result<Self> {
        if !(1..=10).contains(&self.max_results)
            || self.searxng_url.len() > 2048
            || self.searxng_url.chars().any(char::is_control)
        {
            return Err(invalid());
        }
        let url = url::Url::parse(self.searxng_url.trim()).map_err(|_| invalid())?;
        let local = match url.host() {
            Some(url::Host::Domain("localhost")) => true,
            Some(url::Host::Ipv4(ip)) => ip.is_private() || ip.is_loopback(),
            Some(url::Host::Ipv6(ip)) => ip.is_loopback() || ip.is_unique_local(),
            _ => false,
        };
        if !["https", "http"].contains(&url.scheme())
            || url.host().is_none()
            || (!local && url.scheme() != "https")
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(invalid());
        }
        self.searxng_url = url.as_str().trim_end_matches('/').into();
        Ok(self)
    }
}
pub async fn search(config: &Config, value: &str, language: &str) -> Result<Vec<Source>> {
    let config = config.clone().validated()?;
    if config.provider == Provider::DuckDuckGo {
        let mut result = crate::web::search(value, language).await?;
        result.truncate(config.max_results as usize);
        return Ok(result);
    }
    let endpoint = match config.provider {
        Provider::Brave => "https://api.search.brave.com/res/v1/web/search".into(),
        Provider::Tavily => "https://api.tavily.com/search".into(),
        Provider::Searxng => format!("{}/search", config.searxng_url),
        _ => unreachable!(),
    };
    let key = crate::credentials::load_search_key(config.provider)?;
    search_at(&config, &endpoint, key.as_deref(), value, language).await
}
async fn search_at(
    config: &Config,
    endpoint: &str,
    key: Option<&str>,
    value: &str,
    language: &str,
) -> Result<Vec<Source>> {
    let query = crate::web::query(value)?;
    let builder = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(20));
    #[cfg(windows)]
    let builder = builder.use_native_tls();
    let http = builder.build().map_err(|_| invalid())?;
    let mut request=match config.provider{
  Provider::Tavily=>http.post(endpoint).json(&json!({"query":query,"search_depth":"basic","topic":"general","max_results":config.max_results,"include_answer":false,"include_raw_content":false,"include_images":false,"auto_parameters":false})),
  Provider::Brave=>http.get(endpoint).query(&[("q",query.as_str()),("count",&config.max_results.to_string()),("extra_snippets","true"),("search_lang",match language {"zh"=>"zh-hans","ja"=>"jp",other=>other})]),
  Provider::Searxng=>http.get(endpoint).query(&[("q",query.as_str()),("format","json"),("categories","general"),("language",language)]),
  _=>return Err(invalid()),
 };
    if matches!(config.provider, Provider::Brave | Provider::Tavily) {
        let key = key.ok_or_else(|| {
            Error::Validation(
                "Enregistre la clé API de ce moteur de recherche dans Compagnons.".into(),
            )
        })?;
        let mut header =
            reqwest::header::HeaderValue::from_str(&if config.provider == Provider::Tavily {
                format!("Bearer {key}")
            } else {
                key.into()
            })
            .map_err(|_| invalid())?;
        header.set_sensitive(true);
        request = request.header(
            if config.provider == Provider::Brave {
                "X-Subscription-Token"
            } else {
                "Authorization"
            },
            header,
        );
    }
    let mut response = request
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|e| network_failure(&e))?;
    if !response.status().is_success() {
        return Err(Error::Validation(match response.status().as_u16(){401=>"La clé du moteur de recherche est refusée. Vérifie-la dans Compagnons.",429=>"Le quota du moteur de recherche est atteint. Réessaie plus tard ou choisis un autre moteur.",403 if config.provider==Provider::Searxng=>"SearXNG refuse le format JSON. Active-le dans les réglages de ton instance.",402|403 if config.provider==Provider::Brave=>"Brave refuse l’accès. Vérifie la clé, son abonnement Search et son quota dans le tableau de bord Brave.",422=>"Le moteur de recherche refuse les paramètres de la requête. Vérifie sa configuration.",_=>"Le moteur de recherche a refusé la demande. Réessaie plus tard."}.into()));
    }
    let mut bytes = vec![];
    while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
        if bytes.len() + chunk.len() > 1024 * 1024 {
            return Err(invalid());
        }
        bytes.extend(chunk);
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    parse(config.provider, &value, config.max_results as usize)
}
fn plain(s: &str, limit: usize) -> String {
    let text = scraper::Html::parse_fragment(s)
        .root_element()
        .text()
        .collect::<Vec<_>>()
        .join(" ");
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(limit)
        .collect()
}
fn parse(provider: Provider, value: &Value, maximum: usize) -> Result<Vec<Source>> {
    if provider == Provider::Brave && value["type"] == "search" && value["web"].is_null() {
        return Ok(vec![]);
    }
    let rows = if provider == Provider::Brave {
        value["web"]["results"].as_array()
    } else {
        value["results"].as_array()
    }
    .ok_or_else(invalid)?;
    let mut sources: Vec<Source> = vec![];
    for row in rows.iter().take(100) {
        let Some(url) = row["url"].as_str().and_then(crate::web::public_url) else {
            continue;
        };
        if sources.iter().any(|s| s.url == url) {
            continue;
        }
        let title = plain(row["title"].as_str().unwrap_or(""), 240);
        let mut text = row[if provider == Provider::Brave {
            "description"
        } else {
            "content"
        }]
        .as_str()
        .unwrap_or("")
        .to_owned();
        if let Some(extras) = row["extra_snippets"].as_array() {
            for extra in extras.iter().take(3).filter_map(Value::as_str) {
                text.push(' ');
                text.push_str(extra);
            }
        }
        let snippet = plain(&text, 1800);
        if title.is_empty() || snippet.is_empty() {
            continue;
        }
        sources.push(Source {
            title,
            url,
            snippet,
        });
        if sources.len() >= maximum {
            break;
        }
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    #[tokio::test]
    async fn provider_contracts_keep_keys_in_headers_and_normalize_bounded_sources() {
        for provider in [Provider::Brave, Provider::Tavily, Provider::Searxng] {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let endpoint = format!("http://{}/search", listener.local_addr().unwrap());
            let server = tokio::spawn(async move {
                let (mut stream, _) = listener.accept().await.unwrap();
                let mut data = vec![];
                let mut part = [0; 4096];
                loop {
                    let n = stream.read(&mut part).await.unwrap();
                    data.extend_from_slice(&part[..n]);
                    let text = String::from_utf8_lossy(&data);
                    if let Some(end) = text.find("\r\n\r\n") {
                        let count = text[..end]
                            .lines()
                            .find_map(|l| {
                                l.to_lowercase()
                                    .strip_prefix("content-length: ")
                                    .and_then(|s| s.parse::<usize>().ok())
                            })
                            .unwrap_or(0);
                        if data.len() >= end + 4 + count {
                            break;
                        }
                    }
                }
                let text = String::from_utf8(data).unwrap();
                let first = text.lines().next().unwrap();
                assert!(!first.contains("test-search-key"));
                match provider {
                    Provider::Brave => {
                        assert!(
                            text.to_lowercase()
                                .contains("x-subscription-token: test-search-key")
                        );
                        assert!(first.contains("extra_snippets=true"));
                    }
                    Provider::Tavily => {
                        assert!(
                            text.to_lowercase()
                                .contains("authorization: bearer test-search-key")
                        );
                        let body: Value =
                            serde_json::from_str(text.split("\r\n\r\n").nth(1).unwrap()).unwrap();
                        assert_eq!(body["include_answer"], false);
                        assert_eq!(body["max_results"], 3);
                        assert!(!body.to_string().contains("test-search-key"));
                    }
                    Provider::Searxng => assert!(first.contains("format=json")),
                    _ => unreachable!(),
                }
                let rows = json!([{"title":"Official <b>documentation</b>","url":"https://doc.rust-lang.org/book/","description":"Read <b>the book</b>","content":"Read the book","extra_snippets":["Additional evidence"]},{"title":"duplicate","url":"https://doc.rust-lang.org/book/","description":"duplicate","content":"duplicate"},{"title":"local","url":"http://127.0.0.1/","description":"private","content":"private"}]);
                let body = if provider == Provider::Brave {
                    json!({"web":{"results":rows}})
                } else {
                    json!({"results":rows})
                }
                .to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.unwrap();
            });
            let config = Config {
                provider,
                max_results: 3,
                ..Default::default()
            };
            let results = search_at(
                &config,
                &endpoint,
                Some("test-search-key"),
                "Rust documentation",
                "fr",
            )
            .await
            .unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].title, "Official documentation");
            assert!(!results[0].snippet.contains('<'));
            server.await.unwrap();
        }
    }
    #[test]
    fn search_config_rejects_credentials_and_results_outside_the_bound() {
        let c = Config {
            max_results: 11,
            ..Default::default()
        };
        assert!(c.validated().is_err());
        let c = Config {
            searxng_url: "http://user:password@localhost/".into(),
            ..Default::default()
        };
        assert!(c.validated().is_err());
    }
}
