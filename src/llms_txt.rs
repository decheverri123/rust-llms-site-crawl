use url::Url;

const MIN_USEFUL_BYTES: usize = 10;

/// Probe for a site-published `/llms-full.txt`.
///
/// Many static hosts serve their HTML 404 page with a 200 status, so a success
/// status is not sufficient — the body must not look like HTML and must be large
/// enough to plausibly be a docs corpus.
pub async fn probe(client: &reqwest::Client, base: &Url) -> Option<String> {
    let origin = format!("{}://{}", base.scheme(), base.authority());
    for name in ["/llms-full.txt", "/llms.txt"] {
        let resp = client.get(format!("{origin}{name}")).send().await.ok()?;
        if !resp.status().is_success() {
            continue;
        }

        let is_html = resp
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|ct| ct.contains("text/html"));
        if is_html {
            continue;
        }

        let body = resp.text().await.ok()?;
        let head = body.trim_start().to_ascii_lowercase();
        if head.starts_with("<!doctype") || head.starts_with("<html") {
            continue;
        }
        if body.len() < MIN_USEFUL_BYTES {
            continue;
        }

        return Some(body);
    }
    None
}
