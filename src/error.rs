#[derive(Debug, thiserror::Error)]
pub enum WclError {
    #[error("http error for {url}: {source}")]
    Http {
        url: String,
        #[source]
        source: reqwest::Error,
    },
    #[error("failed to parse {what}: {detail}")]
    Parse { what: &'static str, detail: String },
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("no extractable content at {0}")]
    NoContent(String),
    #[error("blocked by robots.txt: {0}")]
    Robots(String),
    #[error("response body for {url} exceeded {limit} byte limit")]
    TooLarge { url: String, limit: u64 },
}
