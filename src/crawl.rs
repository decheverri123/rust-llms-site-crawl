use crate::error::WclError;
use crate::fetch::politeness::Politeness;
use crate::fetch::{Fetcher, HttpFetcher, RawPage};
use crate::markdown::{self, Page};
use crate::options::{DeepStrategy, Options};
use crate::{clean, llms_txt, query, sitemap};
use futures::stream::{self, StreamExt};
use std::collections::{HashSet, VecDeque};
use url::Url;

const QUERY_KEEP_RATIO: f32 = 0.4;

#[derive(Clone)]
pub enum Engine {
    Http(HttpFetcher),
    #[cfg(feature = "browser")]
    Browser(std::sync::Arc<crate::fetch::browser::BrowserFetcher>),
}

impl Engine {
    pub async fn build(opts: &Options) -> Result<Self, WclError> {
        #[cfg(feature = "browser")]
        if opts.browser {
            return Ok(Engine::Browser(std::sync::Arc::new(
                crate::fetch::browser::BrowserFetcher::new(opts).await?,
            )));
        }
        #[cfg(not(feature = "browser"))]
        if opts.browser {
            return Err(WclError::Parse {
                what: "engine",
                detail: "this build has no browser support; install with \
                         `cargo install wcl --features browser`"
                    .into(),
            });
        }
        Ok(Engine::Http(HttpFetcher::new(opts)?))
    }

    pub async fn fetch(&self, url: &Url) -> Result<RawPage, WclError> {
        match self {
            Engine::Http(f) => f.fetch(url).await,
            #[cfg(feature = "browser")]
            Engine::Browser(f) => f.fetch(url).await,
        }
    }
}

/// Transform one fetched page into markdown. Pure: no I/O, no concurrency.
///
/// Link harvesting happens BEFORE chrome stripping because navigation links —
/// exactly what a deep crawl needs — live in the `<nav>` and `<aside>` that
/// stripping removes.
pub fn pipeline(raw: &RawPage, opts: &Options) -> Result<Page, WclError> {
    // Lowered once and shared: link harvesting and title extraction both need a
    // case-insensitive scan of the whole document, and it's cheap only if we
    // don't allocate a second full-document lowercase copy per pass.
    let lower = raw.html.to_ascii_lowercase();
    let links = harvest_links(&raw.html, &lower, &raw.final_url);
    let title = markdown::extract_title_with_lower(&raw.html, &lower);

    let (html, title) = if opts.raw {
        (raw.html.clone(), title)
    } else if opts.prune {
        let d = clean::prune(&raw.html, raw.final_url.as_str())?;
        (d.html, d.title.or(title))
    } else {
        (clean::strip_chrome(&raw.html)?, title)
    };

    let html = match (&opts.css, opts.raw) {
        (Some(sel), false) => clean::scope_to_selector(&html, sel)?,
        _ => html,
    };

    let mut md = markdown::convert(&html, opts)?;
    if let Some(q) = &opts.query {
        md = query::filter_blocks(&md, q, QUERY_KEEP_RATIO);
    }
    if opts.citations {
        md = markdown::apply_citations(&md, &raw.final_url, opts.refs);
    }
    if md.trim().is_empty() {
        return Err(WclError::NoContent(raw.final_url.to_string()));
    }
    let md = markdown::with_metadata(&md, raw.final_url.as_str(), title.as_deref(), opts);
    let tokens = markdown::estimate_tokens(&md);

    Ok(Page {
        url: raw.final_url.to_string(),
        title,
        markdown: md,
        tokens,
        links,
    })
}

fn harvest_links(html: &str, lower: &str, base: &Url) -> Vec<String> {
    let mut links = Vec::new();
    let mut search_from = 0;

    while let Some(pos) = lower[search_from..].find("<a") {
        let a_start = search_from + pos;
        let tag_end = match lower[a_start..].find('>') {
            Some(e) => a_start + e,
            None => break,
        };
        let tag_slice = &html[a_start..tag_end];
        let tag_lower = &lower[a_start..tag_end];

        if let Some(href_pos) = tag_lower.find("href") {
            let after_href = &tag_slice[href_pos + 4..].trim_start();
            if let Some(after_eq) = after_href.strip_prefix('=') {
                let val_part = after_eq.trim_start();
                if let Some(first_char) = val_part.chars().next() {
                    let href = if first_char == '"' || first_char == '\'' {
                        let quote = first_char;
                        let rest = &val_part[1..];
                        match rest.find(quote) {
                            Some(end) => &rest[..end],
                            None => "",
                        }
                    } else {
                        val_part
                            .split_whitespace()
                            .next()
                            .unwrap_or("")
                            .trim_end_matches('>')
                    };

                    if !href.is_empty()
                        && !href.starts_with('#')
                        && !href.starts_with("javascript:")
                        && !href.starts_with("mailto:")
                    {
                        if let Ok(u) = base.join(href) {
                            links.push(u.to_string());
                        }
                    }
                }
            }
        }
        search_from = tag_end + 1;
    }
    links
}

pub async fn run(opts: &Options) -> Result<Vec<Page>, WclError> {
    let base = Url::parse(&opts.url).map_err(|e| WclError::Parse {
        what: "url",
        detail: e.to_string(),
    })?;
    let engine = Engine::build(opts).await?;
    let client = crate::fetch::http::build_client(opts)?;
    let politeness = Politeness::new(client.clone(), opts.ignore_robots, opts.no_delay);

    if opts.site {
        // Fast path: if the user wants the entire site in a single output and no URL filtering is requested,
        // use site-published llms-full.txt if available.
        if opts.match_pattern.is_none() && opts.output_dir.is_none() && opts.max_pages == 0 {
            if let Some(body) = llms_txt::probe(&client, &base).await {
                let tokens = markdown::estimate_tokens(&body);
                return Ok(vec![Page {
                    url: base.to_string(),
                    title: Some(base.host_str().unwrap_or("site").to_string()),
                    markdown: body,
                    tokens,
                    links: Vec::new(),
                }]);
            }
        }
        let seeds = if sitemap::looks_like_sitemap(&opts.url) {
            vec![opts.url.clone()]
        } else {
            sitemap::discover(&politeness, &base).await
        };
        let urls = sitemap::collect_urls(
            &client,
            &base,
            seeds,
            opts.match_pattern.as_deref(),
            opts.page_limit(),
        )
        .await?;
        if urls.is_empty() {
            return Err(WclError::NoContent(format!(
                "no sitemap URLs found for {base}"
            )));
        }
        return fan_out(&engine, &politeness, urls, opts).await;
    }

    if let Some(strategy) = opts.deep {
        return deep_crawl(&engine, &politeness, base, strategy, opts).await;
    }

    let raw = fetch_one(&engine, &politeness, &base).await?;
    Ok(vec![pipeline(&raw, opts)?])
}

async fn fetch_one(e: &Engine, p: &Politeness, url: &Url) -> Result<RawPage, WclError> {
    p.check(url).await?;
    p.wait(url).await;
    e.fetch(url).await
}

async fn fan_out(
    e: &Engine,
    p: &Politeness,
    urls: Vec<Url>,
    opts: &Options,
) -> Result<Vec<Page>, WclError> {
    if let Some(dir) = &opts.output_dir {
        let _ = std::fs::create_dir_all(dir);
    }
    let total = urls.len();
    let bar = indicatif::ProgressBar::new(total as u64);
    let pages: Vec<Page> = stream::iter(urls.into_iter().enumerate())
        .map(|(i, u)| {
            let bar = bar.clone();
            let output_dir = opts.output_dir.clone();
            async move {
                let r = fetch_one(e, p, &u).await.ok()?;
                let page = pipeline(&r, opts).ok()?;
                if let Some(ref dir) = output_dir {
                    let filename = crate::output::page_filename(i, total, &page.url);
                    let _ = std::fs::write(dir.join(filename), &page.markdown);
                }
                bar.inc(1);
                Some(page)
            }
        })
        .buffer_unordered(opts.concurrency)
        .filter_map(|x| async move { x })
        .collect()
        .await;
    bar.finish_and_clear();

    if pages.is_empty() {
        return Err(WclError::NoContent("every page failed".into()));
    }
    Ok(pages)
}

/// Breadth- or depth-first link following, bounded by max_depth and page_limit.
async fn deep_crawl(
    e: &Engine,
    p: &Politeness,
    start: Url,
    strategy: DeepStrategy,
    opts: &Options,
) -> Result<Vec<Page>, WclError> {
    let host = start.host_str().unwrap_or_default().to_string();
    let cap = opts.page_limit().unwrap_or(10);
    let mut frontier: VecDeque<(Url, usize)> = VecDeque::from([(start, 0)]);
    let mut seen: HashSet<String> = HashSet::new();
    let mut pages = Vec::new();

    while let Some((url, depth)) = match strategy {
        DeepStrategy::Bfs => frontier.pop_front(),
        DeepStrategy::Dfs => frontier.pop_back(),
    } {
        if pages.len() >= cap {
            break;
        }
        if !seen.insert(url.to_string()) {
            continue;
        }

        let raw = match fetch_one(e, p, &url).await {
            Ok(r) => r,
            Err(_) => continue,
        };
        let page = match pipeline(&raw, opts) {
            Ok(pg) => pg,
            Err(_) => continue,
        };

        if depth < opts.max_depth {
            for l in &page.links {
                if let Ok(u) = Url::parse(l) {
                    if u.host_str() == Some(&host) && !seen.contains(l) {
                        frontier.push_back((u, depth + 1));
                    }
                }
            }
        }
        pages.push(page);
    }

    if pages.is_empty() {
        return Err(WclError::NoContent("deep crawl produced nothing".into()));
    }
    Ok(pages)
}
