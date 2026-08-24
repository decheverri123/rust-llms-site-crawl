use crate::error::WclError;
use crate::options::Options;
use htmd::options::BulletListMarker;
use htmd::{Element, HtmlToMarkdown};
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Page {
    pub url: String,
    pub title: Option<String>,
    pub markdown: String,
    pub tokens: usize,
    pub links: Vec<String>,
}

/// Fast, high-accuracy BPE token estimator for Markdown and code blocks (~3.8 chars/token)
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        return 0;
    }
    let mut tokens = 0;
    for line in text.lines() {
        if line.is_empty() {
            tokens += 1;
            continue;
        }
        let char_count = line.chars().count();
        let estimated = (char_count as f64 / 3.8).ceil() as usize;
        tokens += estimated.max(1);
    }
    tokens
}

pub fn convert(html: &str, opts: &Options) -> Result<String, WclError> {
    let mut builder = HtmlToMarkdown::builder();
    builder = builder.options(htmd::options::Options {
        bullet_list_marker: BulletListMarker::Dash,
        ul_bullet_spacing: 1,
        ..Default::default()
    });

    let mut skipped = vec!["script", "style"];
    if opts.no_images {
        skipped.push("img");
    }
    builder = builder.skip_tags(skipped);

    if opts.no_links {
        // Unwrap anchors to their text rather than skipping them, which would
        // delete the words too.
        builder = builder.add_handler(
            vec!["a"],
            |handlers: &dyn htmd::element_handler::Handlers, el: Element| {
                Some(handlers.walk_children(el.node).content.into())
            },
        );
    }

    let md = builder.build().convert(html).map_err(|e| WclError::Parse {
        what: "markdown",
        detail: e.to_string(),
    })?;

    let normalized = normalize_blank_lines(&md);
    Ok(normalize_code_fences(&normalized))
}

fn normalize_code_fences(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    // Two-state tracker: when we see an opening fence with no tag, buffer
    // following lines until the matching closing fence, then run the
    // language heuristic over the body and rewrite the opener.
    let mut pending_body: Option<String> = None;
    for line in md.lines() {
        if let Some(rest) = line.strip_prefix("```") {
            let tag = rest.trim();
            let clean_tag = tag
                .strip_prefix("language-")
                .or_else(|| tag.strip_prefix("prism-"))
                .or_else(|| tag.strip_prefix("highlight-"))
                .unwrap_or(tag)
                .split_whitespace()
                .next()
                .unwrap_or("");

            if let Some(mut body) = pending_body.take() {
                // Tag the opener now (already emitted above with empty
                // tag — replace it by retroactively rewriting the last
                // opener line in `out`).
                let guess = crate::language::guess_language(&body);
                if let Some(lang) = guess {
                    // Find the last "```\n" we emitted and re-tag it.
                    if let Some(pos) = out.rfind("```\n") {
                        out.replace_range(pos..pos + 3, &format!("```{lang}\n"));
                    }
                }
                body.push_str(line);
                body.push('\n');
                out.push_str(&body);
                continue;
            }

            if clean_tag.is_empty() {
                // Opening fence with no tag — buffer the body until close.
                pending_body = Some(String::new());
            }
            out.push_str("```");
            out.push_str(clean_tag);
            out.push('\n');
        } else if let Some(buf) = pending_body.as_mut() {
            buf.push_str(line);
            buf.push('\n');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    // Unclosed untagged fence: flush as plain text rather than swallow.
    if let Some(body) = pending_body.take() {
        out.push_str(&body);
    }
    out
}

/// Collapse 3+ consecutive newlines to exactly 2 and trim trailing whitespace
/// per line, so output is byte-stable across htmd versions.
fn normalize_blank_lines(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut blanks = 0usize;
    for line in s.lines() {
        let trimmed = line.trim_end();
        if trimmed.is_empty() {
            blanks += 1;
            if blanks > 1 {
                continue;
            }
        } else {
            blanks = 0;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out.trim_end().to_string() + "\n"
}

pub fn extract_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    extract_title_with_lower(html, &lower)
}

/// Same as [`extract_title`] but takes a pre-lowered copy of `html`, so callers
/// that already lowered the document (e.g. link harvesting) don't pay for it twice.
pub(crate) fn extract_title_with_lower(html: &str, lower: &str) -> Option<String> {
    // Fast path: search for <title> tag
    if let Some(t) = extract_tag_content(html, lower, "title") {
        let stripped = strip_tags(&t);
        let trimmed = trimmed_title(&stripped);
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    // Fallback: search for first <h1> tag
    if let Some(t) = extract_tag_content(html, lower, "h1") {
        let stripped = strip_tags(&t);
        let trimmed = trimmed_title(&stripped);
        if !trimmed.is_empty() {
            return Some(trimmed);
        }
    }
    None
}

fn extract_tag_content(html: &str, lower: &str, tag: &str) -> Option<String> {
    let open_pat = format!("<{tag}");
    let close_pat = format!("</{tag}>");

    let open_pos = lower.find(&open_pat)?;
    let after_open = &html[open_pos..];
    let gt_pos = after_open.find('>')?;
    let start = open_pos + gt_pos + 1;

    let end_rel = lower[start..].find(&close_pat)?;
    Some(html[start..start + end_rel].to_string())
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in s.chars() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            out.push(c);
        }
    }
    out
}

fn trimmed_title(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn with_metadata(body: &str, url: &str, title: Option<&str>, opts: &Options) -> String {
    if opts.no_metadata {
        return body.to_string();
    }
    let tokens = estimate_tokens(body);
    let mut header = String::from("---\n");
    if let Some(t) = title {
        header.push_str(&format!("title: \"{}\"\n", t.replace('"', "\\\"")));
    }
    header.push_str(&format!("source: {url}\n"));
    header.push_str(&format!("tokens: {tokens}\n"));
    header.push_str("---\n\n");
    header + body
}

use url::Url;

/// Rewrite `[text](target)` as `text[n]`, numbering targets in first-appearance
/// order and reusing numbers for repeated targets.
///
/// Walks the string manually instead of using a regex because markdown link
/// syntax is not regular: it must skip image links (`![...]`) and anything inside
/// a backtick code span, both of which a naive pattern would corrupt.
pub fn apply_citations(md: &str, base: &Url, with_refs: bool) -> String {
    let bytes = md.as_bytes();
    let mut out = String::with_capacity(md.len());
    let mut order: Vec<String> = Vec::new();
    let mut seen: HashMap<String, usize> = HashMap::new();
    let mut i = 0usize;
    let mut in_code = false;

    while i < bytes.len() {
        let c = bytes[i] as char;

        if c == '`' {
            in_code = !in_code;
            out.push(c);
            i += 1;
            continue;
        }
        if in_code || c != '[' {
            out.push_str(&md[i..i + char_len(bytes, i)]);
            i += char_len(bytes, i);
            continue;
        }
        // An image is `![...]`; the '!' was already emitted, so check behind.
        if out.ends_with('!') {
            out.push(c);
            i += 1;
            continue;
        }
        match parse_link(md, i) {
            Some((text, target, end)) => {
                let abs = base.join(&target).map(|u| u.to_string()).unwrap_or(target);
                let n = if let Some(&p) = seen.get(&abs) {
                    p
                } else {
                    order.push(abs.clone());
                    let p = order.len();
                    seen.insert(abs, p);
                    p
                };
                out.push_str(&text);
                out.push_str(&format!("[{n}]"));
                i = end;
            }
            None => {
                out.push(c);
                i += 1;
            }
        }
    }

    if with_refs && !order.is_empty() {
        out.push_str("\n\n## References\n\n");
        for (idx, target) in order.iter().enumerate() {
            out.push_str(&format!("[{}]: {}\n", idx + 1, target));
        }
    }
    out
}

fn char_len(bytes: &[u8], i: usize) -> usize {
    let b = bytes[i];
    if b < 0x80 {
        1
    } else if b >> 5 == 0b110 {
        2
    } else if b >> 4 == 0b1110 {
        3
    } else {
        4
    }
}

/// Parse `[text](target)` starting at `start` (which must be '['), returning
/// (text, target, index just past the closing paren).
fn parse_link(md: &str, start: usize) -> Option<(String, String, usize)> {
    let close_b = md[start..].find(']')? + start;
    if md.as_bytes().get(close_b + 1)? != &b'(' {
        return None;
    }
    let close_p = md[close_b + 2..].find(')')? + close_b + 2;
    let text = md[start + 1..close_b].to_string();
    let target = md[close_b + 2..close_p].to_string();
    if target.contains(char::is_whitespace) {
        return None;
    }
    Some((text, target, close_p + 1))
}
