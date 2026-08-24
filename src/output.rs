use crate::error::WclError;
use crate::markdown::Page;
use crate::options::Options;
use std::io::Write;
use url::Url;

/// Derive a filename from a URL's full path, not just its last segment —
/// `/docs/guides/index` and `/docs/api/index` must not collide.
pub fn slug_for(url: &str) -> String {
    let path = Url::parse(url)
        .map(|u| u.path().to_string())
        .unwrap_or_else(|_| url.to_string());
    let s: String = path
        .trim_matches('/')
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    let s = s.trim_matches('-').to_string();
    if s.is_empty() {
        "index".to_string()
    } else {
        s
    }
}

pub fn page_filename(index: usize, total: usize, url: &str) -> String {
    let width = if total >= 1000 {
        4
    } else if total >= 100 {
        3
    } else {
        2
    };
    let slug = slug_for(url);
    format!("{:0width$}_{slug}.md", index + 1, width = width)
}

pub fn build_index(pages: &[Page]) -> String {
    let total_tokens: usize = pages.iter().map(|p| p.tokens).sum();
    let mut out = format!(
        "# Documentation Index\n\n- **Total Pages:** {}\n- **Total Estimated Tokens:** ~{}\n\n",
        pages.len(),
        total_tokens
    );
    for (i, p) in pages.iter().enumerate() {
        let name = p.title.clone().unwrap_or_else(|| p.url.clone());
        let filename = page_filename(i, pages.len(), &p.url);
        let path = url::Url::parse(&p.url)
            .map(|u| u.path().to_string())
            .unwrap_or_else(|_| p.url.clone());
        out.push_str(&format!(
            "- [{}]({}) — `{}` *(~{} tokens)*\n",
            name, filename, path, p.tokens
        ));
    }
    out
}

pub fn build_full_context(pages: &[Page]) -> String {
    let total_tokens: usize = pages.iter().map(|p| p.tokens).sum();
    let mut out = format!(
        "# Full Documentation Context\n\n- Total Documents: {}\n- Total Estimated Tokens: ~{}\n\n",
        pages.len(),
        total_tokens
    );
    for (i, p) in pages.iter().enumerate() {
        let title = p.title.as_deref().unwrap_or("Untitled");
        out.push_str(&format!(
            "<document index=\"{}\" url=\"{}\" title=\"{}\" tokens=\"{}\">\n",
            i + 1,
            p.url,
            title.replace('"', "\\\""),
            p.tokens
        ));
        out.push_str(p.markdown.trim());
        out.push_str("\n</document>\n\n");
    }
    out
}

pub fn build_jsonl(pages: &[Page]) -> String {
    let mut out = String::new();
    for (i, p) in pages.iter().enumerate() {
        let obj = serde_json::json!({
            "index": i + 1,
            "url": p.url,
            "title": p.title,
            "tokens": p.tokens,
            "markdown": p.markdown,
        });
        if let Ok(line) = serde_json::to_string(&obj) {
            out.push_str(&line);
            out.push('\n');
        }
    }
    out
}

pub fn build_llms_txt(pages: &[Page], base_url: Option<&str>) -> String {
    let host = base_url
        .and_then(|u| url::Url::parse(u).ok())
        .and_then(|u| u.host_str().map(|h| h.to_string()))
        .unwrap_or_else(|| "Documentation".to_string());

    let total_tokens: usize = pages.iter().map(|p| p.tokens).sum();
    let mut out = format!(
        "# {host}\n\n> Total documents: {} (~{} tokens)\n\n## Documentation\n\n",
        pages.len(),
        total_tokens
    );
    for (i, p) in pages.iter().enumerate() {
        let name = p.title.clone().unwrap_or_else(|| p.url.clone());
        let filename = page_filename(i, pages.len(), &p.url);
        let path = url::Url::parse(&p.url)
            .map(|u| u.path().to_string())
            .unwrap_or_else(|_| p.url.clone());
        out.push_str(&format!(
            "- [{}]({}): {} (~{} tokens)\n",
            name, filename, path, p.tokens
        ));
    }
    out
}

pub fn copy_to_clipboard(text: &str) -> Result<(), WclError> {
    match arboard::Clipboard::new() {
        Ok(mut clipboard) => {
            if let Err(e) = clipboard.set_text(text) {
                eprintln!("warning: failed to set system clipboard: {e}");
            } else {
                eprintln!("✓ Copied markdown directly to clipboard");
            }
        }
        Err(e) => {
            eprintln!("warning: clipboard unavailable: {e}");
        }
    }
    Ok(())
}

pub fn render(pages: &[Page], opts: &Options) -> Result<(), WclError> {
    if let Some(dir) = &opts.output_dir {
        std::fs::create_dir_all(dir)?;
        for (i, p) in pages.iter().enumerate() {
            let filename = page_filename(i, pages.len(), &p.url);
            let target = dir.join(filename);
            if !target.exists() {
                std::fs::write(target, &p.markdown)?;
            }
        }
        let index_content = build_index(pages);
        std::fs::write(dir.join("00_INDEX.md"), &index_content)?;
        std::fs::write(dir.join("llms.txt"), build_llms_txt(pages, Some(&opts.url)))?;
        std::fs::write(dir.join("FULL_CONTEXT.md"), build_full_context(pages))?;
        std::fs::write(dir.join("docs.jsonl"), build_jsonl(pages))?;

        if opts.copy {
            let full = build_full_context(pages);
            let _ = copy_to_clipboard(&full);
        }

        let total_tokens: usize = pages.iter().map(|p| p.tokens).sum();
        eprintln!(
            "wrote {} pages (~{} tokens) to {}",
            pages.len(),
            total_tokens,
            dir.display()
        );
        return Ok(());
    }

    if opts.jsonl {
        let jsonl_content = build_jsonl(pages);
        match &opts.output {
            Some(path) => {
                std::fs::write(path, &jsonl_content)?;
                eprintln!("wrote JSONL to {}", path.display());
            }
            None => {
                let stdout = std::io::stdout();
                let mut lock = stdout.lock();
                write!(lock, "{jsonl_content}")?;
            }
        }
        return Ok(());
    }

    let body = pages
        .iter()
        .map(|p| p.markdown.trim_end().to_string())
        .collect::<Vec<_>>()
        .join("\n\n---\n\n");

    if opts.copy {
        let _ = copy_to_clipboard(&body);
    }

    match &opts.output {
        Some(path) => {
            std::fs::write(path, body + "\n")?;
            eprintln!("wrote {} pages to {}", pages.len(), path.display());
        }
        None => {
            let stdout = std::io::stdout();
            let mut lock = stdout.lock();
            writeln!(lock, "{body}")?;
        }
    }
    Ok(())
}
