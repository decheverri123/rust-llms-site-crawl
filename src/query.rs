use bm25::{EmbedderBuilder, Language, Scorer};

/// Split markdown into blocks at blank lines, score each against `query` with
/// BM25, and keep the top `keep_ratio` fraction in document order.
///
/// Headings are attached to the block that follows them so a kept paragraph
/// never loses its section title.
pub fn filter_blocks(md: &str, query: &str, keep_ratio: f32) -> String {
    if query.trim().is_empty() || keep_ratio >= 1.0 {
        return md.to_string();
    }
    let blocks = split_blocks(md);
    if blocks.len() <= 1 {
        return md.to_string();
    }

    let embedder = EmbedderBuilder::<u32>::with_fit_to_corpus(
        Language::English,
        &blocks.iter().map(String::as_str).collect::<Vec<_>>(),
    )
    .build();

    let mut scorer = Scorer::<u32>::new();
    for (i, b) in blocks.iter().enumerate() {
        scorer.upsert(&(i as u32), embedder.embed(b));
    }
    let q = embedder.embed(query);

    let mut scored: Vec<(usize, f32)> = (0..blocks.len())
        .map(|i| (i, scorer.score(&(i as u32), &q).unwrap_or(0.0)))
        .collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));

    let keep_n = ((blocks.len() as f32 * keep_ratio).round() as usize).max(1);
    let mut keep: Vec<usize> = scored.into_iter().take(keep_n).map(|(i, _)| i).collect();
    keep.sort_unstable();

    keep.into_iter()
        .map(|i| blocks[i].clone())
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn split_blocks(md: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut pending_heading: Option<String> = None;

    for raw in md.split("\n\n") {
        let b = raw.trim();
        if b.is_empty() {
            continue;
        }
        if b.lines().count() == 1 && b.starts_with('#') {
            pending_heading = Some(b.to_string());
            continue;
        }
        match pending_heading.take() {
            Some(h) => blocks.push(format!("{h}\n\n{b}")),
            None => blocks.push(b.to_string()),
        }
    }
    if let Some(h) = pending_heading {
        blocks.push(h);
    }
    blocks
}
