use super::{Fetcher, RawPage};
use crate::error::WclError;
use crate::options::Options;
use encoding_rs::{Encoding, UTF_8};
use futures::StreamExt;
use std::time::Duration;
use url::Url;

pub const USER_AGENT: &str = concat!(
    "wcl/",
    env!("CARGO_PKG_VERSION"),
    " (+https://github.com/decheverri123/rust-llms-site-crawl)"
);
const MAX_RETRIES: u32 = 3;
/// Cap on decompressed body size. Gzip/brotli are decoded transparently by
/// reqwest, so a malicious server can serve a tiny compressed payload that
/// expands far past this if left unchecked — stream-and-count instead of
/// buffering the whole body first.
pub const MAX_BODY_BYTES: u64 = 128 * 1024 * 1024;

pub(crate) async fn read_capped_body(
    resp: reqwest::Response,
    url: &str,
    limit: u64,
) -> Result<Vec<u8>, WclError> {
    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| WclError::Http {
            url: url.to_string(),
            source: e,
        })?;
        if body.len() as u64 + chunk.len() as u64 > limit {
            return Err(WclError::TooLarge {
                url: url.to_string(),
                limit,
            });
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub fn build_client(opts: &Options) -> Result<reqwest::Client, WclError> {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_millis(opts.timeout_ms))
        .redirect(reqwest::redirect::Policy::limited(10))
        .tcp_nodelay(true)
        .tcp_keepalive(Duration::from_secs(60))
        .http2_adaptive_window(true)
        .pool_idle_timeout(Duration::from_secs(90))
        .pool_max_idle_per_host(opts.concurrency)
        .gzip(true)
        .brotli(true)
        .build()
        .map_err(|e| WclError::Http {
            url: opts.url.clone(),
            source: e,
        })
}

#[derive(Clone)]
pub struct HttpFetcher {
    client: reqwest::Client,
}

impl HttpFetcher {
    pub fn new(opts: &Options) -> Result<Self, WclError> {
        let client = build_client(opts)?;
        Ok(Self { client })
    }
}

/// Resolve the byte encoding: Content-Type header wins, then a `<meta charset>`
/// in the first 1 KiB, then UTF-8. Servers lie about charset often enough that
/// the meta fallback is load-bearing, not defensive padding.
fn resolve_encoding(content_type: Option<&str>, body: &[u8]) -> &'static Encoding {
    if let Some(ct) = content_type {
        if let Some(cs) = ct
            .split(';')
            .filter_map(|p| p.trim().strip_prefix("charset="))
            .next()
        {
            if let Some(enc) = Encoding::for_label(cs.trim_matches('"').as_bytes()) {
                return enc;
            }
        }
    }
    let head = &body[..body.len().min(1024)];
    let text = String::from_utf8_lossy(head);
    if let Some(idx) = text.find("charset=") {
        let rest = &text[idx + 8..];
        let label: String = rest
            .trim_start_matches(['"', '\''])
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
            .collect();
        if let Some(enc) = Encoding::for_label(label.as_bytes()) {
            return enc;
        }
    }
    UTF_8
}

fn is_retryable(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

impl Fetcher for HttpFetcher {
    async fn fetch(&self, url: &Url) -> Result<RawPage, WclError> {
        let mut attempt = 0;
        loop {
            let resp = self
                .client
                .get(url.clone())
                .send()
                .await
                .map_err(|e| WclError::Http {
                    url: url.to_string(),
                    source: e,
                })?;
            let status = resp.status().as_u16();

            if is_retryable(status) && attempt < MAX_RETRIES {
                let backoff = Duration::from_millis(250 * 2u64.pow(attempt));
                tokio::time::sleep(backoff).await;
                attempt += 1;
                continue;
            }

            let final_url = resp.url().clone();
            let content_type = resp
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned);
            let bytes = read_capped_body(resp, url.as_str(), MAX_BODY_BYTES).await?;
            let enc = resolve_encoding(content_type.as_deref(), &bytes);
            let (html, _, _) = enc.decode(&bytes);

            return Ok(RawPage {
                requested_url: url.clone(),
                final_url,
                status,
                html: html.into_owned(),
                content_type,
            });
        }
    }
}
