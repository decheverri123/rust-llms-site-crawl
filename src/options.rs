use std::path::PathBuf;

pub const MAX_PER_HOST_CONCURRENCY: usize = 256;
pub const MAX_GLOBAL_CONCURRENCY: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeepStrategy {
    Bfs,
    Dfs,
}

#[derive(Debug, Clone)]
pub struct Options {
    pub url: String,
    pub raw: bool,
    pub prune: bool,
    pub citations: bool,
    pub refs: bool,
    pub query: Option<String>,
    pub css: Option<String>,
    pub full: bool,
    pub wait_for: Option<String>,
    pub no_links: bool,
    pub no_images: bool,
    pub timeout_ms: u64,
    pub site: bool,
    pub match_pattern: Option<String>,
    pub deep: Option<DeepStrategy>,
    pub max_pages: usize,
    pub max_depth: usize,
    pub concurrency: usize,
    pub browser: bool,
    pub ignore_robots: bool,
    pub no_delay: bool,
    pub no_metadata: bool,
    pub verbose: bool,
    pub tui: bool,
    pub copy: bool,
    pub jsonl: bool,
    pub output: Option<PathBuf>,
    pub output_dir: Option<PathBuf>,
}

impl Options {
    /// `--site` with no explicit `--pages` means "every page in the sitemap".
    pub fn page_limit(&self) -> Option<usize> {
        if self.max_pages == 0 {
            None
        } else {
            Some(self.max_pages)
        }
    }

    pub fn for_test(url: &str) -> Self {
        Options {
            url: url.to_string(),
            raw: false,
            prune: false,
            citations: false,
            refs: false,
            query: None,
            css: None,
            full: false,
            wait_for: None,
            no_links: false,
            no_images: false,
            timeout_ms: 5_000,
            site: false,
            match_pattern: None,
            deep: None,
            max_pages: 0,
            max_depth: 2,
            concurrency: 4,
            browser: false,
            ignore_robots: true,
            no_delay: true,
            no_metadata: true,
            verbose: false,
            tui: false,
            copy: false,
            jsonl: false,
            output: None,
            output_dir: None,
        }
    }
}
