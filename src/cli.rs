use crate::options::{DeepStrategy, Options, MAX_PER_HOST_CONCURRENCY};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "wcl",
    version,
    about = "Turn any website into LLM-ready markdown"
)]
pub struct Args {
    /// URL to crawl (omit to launch interactive TUI)
    pub url: Option<String>,

    /// Launch the interactive Terminal UI (TUI) dashboard
    #[arg(long)]
    pub tui: bool,

    #[arg(short = 'o', long = "output")]
    pub output: Option<PathBuf>,
    #[arg(short = 'O', long = "output-dir")]
    pub output_dir: Option<PathBuf>,

    /// Crawl every page on the site via sitemap discovery
    #[arg(long)]
    pub site: bool,
    /// Keep only sitemap URLs containing this substring
    #[arg(long = "match")]
    pub match_pattern: Option<String>,

    /// Aggressive noise removal (readability extraction)
    #[arg(long)]
    pub prune: bool,
    /// Query-focused extraction (BM25)
    #[arg(short = 'q', long)]
    pub query: Option<String>,
    /// Convert inline links to numbered citations
    #[arg(long)]
    pub citations: bool,
    /// Append a references section (implies --citations)
    #[arg(long)]
    pub refs: bool,
    /// No chrome stripping, no filtering — the page untouched
    #[arg(long)]
    pub raw: bool,
    /// Scope extraction to a CSS selector
    #[arg(long)]
    pub css: Option<String>,
    /// Scroll the full page for lazy-loaded content (implies --browser)
    #[arg(long)]
    pub full: bool,
    /// CSS selector to wait for before extracting (implies --browser)
    #[arg(long = "wait-for")]
    pub wait_for: Option<String>,

    #[arg(long, value_parser = ["bfs", "dfs"])]
    pub deep: Option<String>,
    #[arg(long = "pages", default_value_t = 0)]
    pub max_pages: usize,
    #[arg(long = "max-depth", default_value_t = 2)]
    pub max_depth: usize,

    #[arg(long = "no-links")]
    pub no_links: bool,
    #[arg(long = "no-images")]
    pub no_images: bool,
    #[arg(long, default_value_t = 30_000)]
    pub timeout: u64,
    #[arg(long, default_value_t = 16)]
    pub concurrency: usize,

    /// Use headless Chrome instead of plain HTTP
    #[arg(long)]
    pub browser: bool,
    /// Ignore robots.txt (prints a warning)
    #[arg(long = "ignore-robots")]
    pub ignore_robots: bool,
    /// Skip Crawl-delay waits from robots.txt (does not bypass allow/disallow rules)
    #[arg(long = "no-delay")]
    pub no_delay: bool,
    /// Omit the source/title/time metadata header
    #[arg(long = "no-metadata")]
    pub no_metadata: bool,
    /// Copy output markdown directly to clipboard
    #[arg(short = 'c', long = "copy")]
    pub copy: bool,
    /// Export crawled pages to JSON Lines (.jsonl)
    #[arg(long = "jsonl")]
    pub jsonl: bool,

    #[arg(short, long)]
    pub verbose: bool,

    /// Print resolved options and exit (test hook)
    #[arg(long = "print-options", hide = true)]
    pub print_options: bool,
}

pub fn parse() -> (Options, bool) {
    let a = Args::parse();
    let is_tui = a.tui || a.url.is_none();
    let url = a.url.unwrap_or_default();
    let opts = Options {
        url,
        raw: a.raw,
        prune: a.prune,
        citations: a.citations || a.refs,
        refs: a.refs,
        query: a.query,
        css: a.css,
        full: a.full,
        wait_for: a.wait_for.clone(),
        no_links: a.no_links,
        no_images: a.no_images,
        timeout_ms: a.timeout,
        site: a.site,
        match_pattern: a.match_pattern,
        deep: match a.deep.as_deref() {
            Some("bfs") => Some(DeepStrategy::Bfs),
            Some("dfs") => Some(DeepStrategy::Dfs),
            _ => None,
        },
        max_pages: a.max_pages,
        max_depth: a.max_depth,
        concurrency: a.concurrency.clamp(1, MAX_PER_HOST_CONCURRENCY),
        browser: a.browser || a.full || a.wait_for.is_some(),
        ignore_robots: a.ignore_robots,
        no_delay: a.no_delay,
        no_metadata: a.no_metadata,
        verbose: a.verbose,
        tui: is_tui,
        copy: a.copy,
        jsonl: a.jsonl,
        output: a.output,
        output_dir: a.output_dir,
    };
    (opts, a.print_options)
}
