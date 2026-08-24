use crate::error::WclError;
use crate::fetch::http::{read_capped_body, MAX_BODY_BYTES};
use crate::fetch::politeness::Politeness;
use quick_xml::events::Event;
use quick_xml::Reader;
use std::collections::HashSet;
use url::Url;

const MAX_SITEMAPS: usize = 200;

pub fn looks_like_sitemap(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.ends_with(".xml") || lower.contains("sitemap")
}

/// Candidate sitemap locations, robots.txt directives first.
pub async fn discover(politeness: &Politeness, base: &Url) -> Vec<String> {
    let origin = format!("{}://{}", base.scheme(), base.authority());
    let mut out = politeness.sitemaps_for(base).await;
    for p in [
        "/sitemap.xml",
        "/sitemap_index.xml",
        "/sitemap-index.xml",
        "/sitemaps.xml",
    ] {
        let c = format!("{origin}{p}");
        if !out.contains(&c) {
            out.push(c);
        }
    }
    out
}

/// Returns (page urls, sub-sitemap urls). Handles namespaced and bare tags, and
/// falls back to scanning for `<loc>` when the XML does not parse — real
/// sitemaps in the wild are frequently malformed.
pub fn parse_xml(bytes: &[u8]) -> (Vec<String>, Vec<String>) {
    let mut urls = Vec::new();
    let mut subs = Vec::new();
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);

    let mut in_sitemap_entry = false;
    let mut in_loc = false;
    let mut buf = Vec::new();
    let mut ok = true;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => match local_name(e.name().as_ref()) {
                "sitemap" => in_sitemap_entry = true,
                "url" => in_sitemap_entry = false,
                "loc" => in_loc = true,
                _ => {}
            },
            Ok(Event::End(e)) => match local_name(e.name().as_ref()) {
                "sitemap" => in_sitemap_entry = false,
                "loc" => in_loc = false,
                _ => {}
            },
            Ok(Event::Text(t)) if in_loc => {
                let raw = t.as_ref();
                let unescaped =
                    quick_xml::escape::unescape(raw).unwrap_or(std::borrow::Cow::Borrowed(raw));
                let s = unescaped.trim().to_string();
                if !s.is_empty() {
                    if in_sitemap_entry {
                        subs.push(s)
                    } else {
                        urls.push(s)
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => {
                ok = false;
                break;
            }
            _ => {}
        }
        buf.clear();
    }

    if !ok && urls.is_empty() && subs.is_empty() {
        let text = String::from_utf8_lossy(bytes);
        for seg in text.split("<loc>").skip(1) {
            if let Some(end) = seg.find("</loc>") {
                let s = seg[..end].trim().to_string();
                if s.starts_with("http") {
                    if looks_like_sitemap(&s) {
                        subs.push(s)
                    } else {
                        urls.push(s)
                    }
                }
            }
        }
    }
    (urls, subs)
}

fn local_name(full: &str) -> &str {
    if let Some(pos) = full.rfind(':') {
        &full[pos + 1..]
    } else {
        full
    }
}

/// Same registrable-site check: exact host match, or a subdomain of the base host.
pub fn filter_same_site(urls: &[String], base: &Url, match_pattern: Option<&str>) -> Vec<Url> {
    let host = base.host_str().unwrap_or_default().to_string();
    urls.iter()
        .filter_map(|u| Url::parse(u).ok())
        .filter(|u| {
            let h = u.host_str().unwrap_or_default();
            h == host || h.ends_with(&format!(".{host}"))
        })
        .filter(|u| match_pattern.is_none_or(|p| u.as_str().contains(p)))
        .collect()
}

pub async fn collect_urls(
    client: &reqwest::Client,
    base: &Url,
    seeds: Vec<String>,
    match_pattern: Option<&str>,
    limit: Option<usize>,
) -> Result<Vec<Url>, WclError> {
    let mut queue: Vec<String> = seeds;
    let mut seen_maps: HashSet<String> = HashSet::new();
    let mut seen_urls: HashSet<String> = HashSet::new();
    let mut out: Vec<Url> = Vec::new();

    while let Some(map_url) = queue.pop() {
        if seen_maps.len() >= MAX_SITEMAPS {
            break;
        }
        if !seen_maps.insert(map_url.clone()) {
            continue;
        }

        let bytes = match client.get(&map_url).send().await {
            Ok(r) if r.status().is_success() => {
                match read_capped_body(r, &map_url, MAX_BODY_BYTES).await {
                    Ok(b) => b,
                    Err(_) => continue,
                }
            }
            _ => continue,
        };
        let bytes = maybe_gunzip(&map_url, bytes);
        let (page_urls, subs) = parse_xml(&bytes);

        for s in subs {
            if !seen_maps.contains(&s) {
                queue.push(s);
            }
        }
        for u in filter_same_site(&page_urls, base, match_pattern) {
            if seen_urls.insert(u.to_string()) {
                out.push(u);
                if limit.is_some_and(|n| out.len() >= n) {
                    return Ok(out);
                }
            }
        }
    }
    Ok(out)
}

fn maybe_gunzip(url: &str, bytes: Vec<u8>) -> Vec<u8> {
    let gzipped = url.ends_with(".gz") || bytes.starts_with(&[0x1f, 0x8b]);
    if !gzipped {
        return bytes;
    }
    use std::io::Read;
    let d = flate2::read::GzDecoder::new(&bytes[..]);
    let mut out = Vec::new();
    match d.take(MAX_BODY_BYTES).read_to_end(&mut out) {
        Ok(_) => out,
        Err(_) => bytes,
    }
}
