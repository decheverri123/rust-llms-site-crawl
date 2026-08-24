#[cfg(feature = "browser")]
pub mod browser;
pub mod http;
pub mod politeness;

pub use http::HttpFetcher;

use crate::error::WclError;
use url::Url;

#[derive(Debug, Clone)]
pub struct RawPage {
    pub requested_url: Url,
    pub final_url: Url,
    pub status: u16,
    pub html: String,
    pub content_type: Option<String>,
}

pub trait Fetcher {
    fn fetch(
        &self,
        url: &Url,
    ) -> impl std::future::Future<Output = Result<RawPage, WclError>> + Send;
}
