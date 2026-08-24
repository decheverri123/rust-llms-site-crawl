use crate::error::WclError;
use lol_html::{element, rewrite_str, RewriteStrSettings};

/// Structural boilerplate removed before markdown generation.
///
/// `header` is deliberately absent: Docusaurus, VitePress and Hugo all render the
/// page hero (the H1 and its subtitle) inside `<header>`, so stripping it loses
/// the page title on the exact sites this tool targets most.
pub const CHROME_TAGS: &[&str] = &[
    "nav",
    "footer",
    "aside",
    "form",
    "style",
    "script",
    "noscript",
    "button",
    ".pagination-nav",
    ".theme-doc-footer",
    ".theme-doc-toc-mobile",
    "[class*='copyButton']",
    "[class*='feedback-prompt']",
    "[class*='was-this-helpful']",
];

pub fn strip_chrome(html: &str) -> Result<String, WclError> {
    let mut settings = RewriteStrSettings::new();
    for tag in CHROME_TAGS {
        settings = settings.append_element_content_handler(element!(*tag, |el| {
            el.remove();
            Ok(())
        }));
    }

    rewrite_str(html, settings).map_err(|e| WclError::Parse {
        what: "html",
        detail: e.to_string(),
    })
}

/// Keep only the subtrees matching `selector`, concatenated in document order.
///
/// Uses `scraper` rather than `lol_html` here: lol_html is a streaming rewriter
/// and cannot express "discard everything that did not match", which needs a
/// materialized tree.
pub fn scope_to_selector(html: &str, selector: &str) -> Result<String, WclError> {
    use scraper::{Html, Selector};

    let sel = Selector::parse(selector).map_err(|e| WclError::Parse {
        what: "css selector",
        detail: format!("{e:?}"),
    })?;
    let doc = Html::parse_document(html);
    let parts: Vec<String> = doc.select(&sel).map(|el| el.html()).collect();

    if parts.is_empty() {
        return Err(WclError::NoContent(format!(
            "selector matched nothing: {selector}"
        )));
    }
    Ok(parts.join("\n"))
}

#[derive(Debug, Clone)]
pub struct PrunedDoc {
    pub title: Option<String>,
    pub html: String,
}

/// Readability-style main-content extraction.
///
/// `dom_smoothie` is a port of Mozilla's Readability. It scores candidate blocks
/// by text density and link ratio, which is why `--prune` can drop legitimate
/// content on link-heavy index pages — that tradeoff is why it stays opt-in.
pub fn prune(html: &str, url: &str) -> Result<PrunedDoc, WclError> {
    use dom_smoothie::{Config, Readability};

    let cfg = Config {
        ..Default::default()
    };
    let mut r = Readability::new(html, Some(url), Some(cfg)).map_err(|e| WclError::Parse {
        what: "readability",
        detail: e.to_string(),
    })?;
    let article = r.parse().map_err(|e| WclError::Parse {
        what: "readability",
        detail: e.to_string(),
    })?;

    let body = article.content.to_string();
    if body.trim().is_empty() {
        return Err(WclError::NoContent(url.to_string()));
    }
    let title = if article.title.trim().is_empty() {
        None
    } else {
        Some(article.title)
    };
    Ok(PrunedDoc { title, html: body })
}
