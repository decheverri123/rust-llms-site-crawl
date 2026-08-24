use super::{Fetcher, RawPage};
use crate::error::WclError;
use crate::options::Options;
use chromiumoxide::browser::{Browser, BrowserConfig};
use futures::StreamExt;
use std::time::Duration;
use url::Url;

pub struct BrowserFetcher {
    browser: Browser,
    scan_full_page: bool,
    wait_for: Option<String>,
    timeout: Duration,
    _handler: tokio::task::JoinHandle<()>,
}

impl BrowserFetcher {
    pub async fn new(opts: &Options) -> Result<Self, WclError> {
        let config = BrowserConfig::builder()
            .request_timeout(Duration::from_millis(opts.timeout_ms))
            .build()
            .map_err(|e| WclError::Parse {
                what: "browser config",
                detail: e,
            })?;

        let (browser, mut handler) =
            Browser::launch(config).await.map_err(|e| WclError::Parse {
                what: "browser launch",
                detail: e.to_string(),
            })?;

        // chromiumoxide requires the event handler to be driven for the whole
        // session; dropping this task deadlocks every subsequent page call.
        let _handler = tokio::spawn(async move { while handler.next().await.is_some() {} });

        Ok(Self {
            browser,
            scan_full_page: opts.full,
            wait_for: opts.wait_for.clone(),
            timeout: Duration::from_millis(opts.timeout_ms),
            _handler,
        })
    }
}

impl Fetcher for BrowserFetcher {
    async fn fetch(&self, url: &Url) -> Result<RawPage, WclError> {
        let err = |e: chromiumoxide::error::CdpError| WclError::Parse {
            what: "browser fetch",
            detail: e.to_string(),
        };

        let page = self.browser.new_page(url.as_str()).await.map_err(err)?;
        page.wait_for_navigation().await.map_err(err)?;

        if let Some(sel) = &self.wait_for {
            let deadline = tokio::time::Instant::now() + self.timeout;
            while page.find_element(sel).await.is_err() {
                if tokio::time::Instant::now() > deadline {
                    return Err(WclError::NoContent(format!("wait-for timed out: {sel}")));
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }

        if self.scan_full_page {
            // Trigger lazy-loading by scrolling to the bottom in steps, then
            // returning to the top so no viewport-gated content is skipped.
            for _ in 0..20 {
                page.evaluate("window.scrollBy(0, window.innerHeight)")
                    .await
                    .map_err(err)?;
                tokio::time::sleep(Duration::from_millis(150)).await;
            }
            page.evaluate("window.scrollTo(0, 0)").await.map_err(err)?;
        }

        let html = page.content().await.map_err(err)?;
        let final_url = page
            .url()
            .await
            .map_err(err)?
            .and_then(|u| Url::parse(&u).ok())
            .unwrap_or_else(|| url.clone());
        let _ = page.close().await;

        Ok(RawPage {
            requested_url: url.clone(),
            final_url,
            status: 200,
            html,
            content_type: Some("text/html".into()),
        })
    }
}
