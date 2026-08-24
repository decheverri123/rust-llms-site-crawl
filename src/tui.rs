use crate::markdown::Page;
use crate::options::Options;
use crate::output;
use crossterm::{
    event::{Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Gauge, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};
use std::io::{self, Stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use url::Url;

pub enum CrawlEvent {
    Started { total: usize },
    PageDone(Page),
    Error(String),
    Finished,
}

#[derive(PartialEq)]
enum TuiState {
    Config,
    Crawling,
    Completed,
}

#[derive(PartialEq, Clone, Copy)]
enum ActiveInput {
    Url,
    Match,
    OutputDir,
    Concurrency,
    MaxPages,
    Query,
    Css,
    Flags,
}

pub struct TuiApp {
    state: TuiState,
    opts: Options,
    // Config form inputs
    input_url: String,
    input_match: String,
    input_output_dir: String,
    input_concurrency: String,
    input_max_pages: String,
    input_query: String,
    input_css: String,
    // Toggleable CLI flags
    flag_site: bool,
    flag_prune: bool,
    flag_citations: bool,
    flag_refs: bool,
    flag_raw: bool,
    flag_browser: bool,
    flag_ignore_robots: bool,
    flag_no_delay: bool,
    flag_no_links: bool,
    flag_no_images: bool,
    flag_no_metadata: bool,
    active_input: ActiveInput,
    selected_flag_index: usize,
    show_args_modal: bool,
    // Live search & filter
    search_query: String,
    is_searching: bool,
    // Crawl live state
    start_time: Instant,
    completed_duration: Option<Duration>,
    total_pages: usize,
    processed_count: usize,
    pages: Vec<Page>,
    list_state: ListState,
    preview_scroll: u16,
    status_msg: String,
    error_count: usize,
    needs_restart: bool,
}

impl TuiApp {
    pub fn new(opts: Options) -> Self {
        let is_configured = !opts.url.trim().is_empty();
        let state = if is_configured {
            TuiState::Crawling
        } else {
            TuiState::Config
        };

        let mut list_state = ListState::default();
        list_state.select(Some(0));

        let input_url = opts.url.clone();
        let input_match = opts.match_pattern.clone().unwrap_or_default();
        let input_output_dir = opts
            .output_dir
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        let input_concurrency = opts.concurrency.to_string();
        let input_max_pages = if opts.max_pages > 0 {
            opts.max_pages.to_string()
        } else {
            "".to_string()
        };
        let input_query = opts.query.clone().unwrap_or_default();
        let input_css = opts.css.clone().unwrap_or_default();

        Self {
            state,
            flag_site: opts.site,
            flag_prune: opts.prune,
            flag_citations: opts.citations,
            flag_refs: opts.refs,
            flag_raw: opts.raw,
            flag_browser: opts.browser,
            flag_ignore_robots: opts.ignore_robots,
            flag_no_delay: opts.no_delay,
            flag_no_links: opts.no_links,
            flag_no_images: opts.no_images,
            flag_no_metadata: opts.no_metadata,
            opts,
            input_url,
            input_match,
            input_output_dir,
            input_concurrency,
            input_max_pages,
            input_query,
            input_css,
            active_input: ActiveInput::Url,
            selected_flag_index: 0,
            show_args_modal: false,
            search_query: String::new(),
            is_searching: false,
            start_time: Instant::now(),
            completed_duration: None,
            total_pages: 0,
            processed_count: 0,
            pages: Vec::new(),
            list_state,
            preview_scroll: 0,
            status_msg: "Ready".to_string(),
            error_count: 0,
            needs_restart: false,
        }
    }

    pub fn apply_config(&mut self) {
        self.opts.url = self.input_url.trim().to_string();
        self.opts.site = self.flag_site;
        self.opts.prune = self.flag_prune;
        self.opts.citations = self.flag_citations;
        self.opts.refs = self.flag_refs;
        self.opts.raw = self.flag_raw;
        self.opts.browser = self.flag_browser;
        self.opts.ignore_robots = self.flag_ignore_robots;
        self.opts.no_delay = self.flag_no_delay;
        self.opts.no_links = self.flag_no_links;
        self.opts.no_images = self.flag_no_images;
        self.opts.no_metadata = self.flag_no_metadata;
        self.opts.match_pattern = if self.input_match.trim().is_empty() {
            None
        } else {
            Some(self.input_match.trim().to_string())
        };
        self.opts.output_dir = if self.input_output_dir.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(self.input_output_dir.trim()))
        };
        self.opts.query = if self.input_query.trim().is_empty() {
            None
        } else {
            Some(self.input_query.trim().to_string())
        };
        self.opts.css = if self.input_css.trim().is_empty() {
            None
        } else {
            Some(self.input_css.trim().to_string())
        };
        self.opts.max_pages = self.input_max_pages.trim().parse().unwrap_or(0);
        self.opts.concurrency = self
            .input_concurrency
            .trim()
            .parse()
            .unwrap_or(16)
            .clamp(1, 256);
        self.state = TuiState::Crawling;
        self.start_time = Instant::now();
        self.completed_duration = None;
        self.processed_count = 0;
        self.error_count = 0;
        self.pages = Vec::new();
        self.list_state = ListState::default();
        self.list_state.select(Some(0));
        self.needs_restart = true;
    }

    pub fn total_tokens(&self) -> usize {
        self.pages.iter().map(|p| p.tokens).sum()
    }

    pub fn filtered_indices(&self) -> Vec<usize> {
        if self.search_query.trim().is_empty() {
            (0..self.pages.len()).collect()
        } else {
            let q = self.search_query.to_lowercase();
            self.pages
                .iter()
                .enumerate()
                .filter(|(_, p)| {
                    p.url.to_lowercase().contains(&q)
                        || p.title
                            .as_deref()
                            .map(|t| t.to_lowercase().contains(&q))
                            .unwrap_or(false)
                })
                .map(|(i, _)| i)
                .collect()
        }
    }

    pub fn active_text_mut(&mut self) -> Option<&mut String> {
        match self.active_input {
            ActiveInput::Url => Some(&mut self.input_url),
            ActiveInput::Match => Some(&mut self.input_match),
            ActiveInput::OutputDir => Some(&mut self.input_output_dir),
            ActiveInput::Concurrency => Some(&mut self.input_concurrency),
            ActiveInput::MaxPages => Some(&mut self.input_max_pages),
            ActiveInput::Query => Some(&mut self.input_query),
            ActiveInput::Css => Some(&mut self.input_css),
            ActiveInput::Flags => None,
        }
    }

    pub fn clear_active_input(&mut self) {
        if let Some(s) = self.active_text_mut() {
            s.clear();
        }
    }

    pub fn delete_word_back(&mut self) {
        if let Some(s) = self.active_text_mut() {
            while s.ends_with(char::is_whitespace) {
                s.pop();
            }
            while let Some(c) = s.chars().last() {
                if c.is_whitespace() || c == '/' || c == ':' || c == '.' || c == '-' || c == '_' {
                    s.pop();
                    break;
                }
                s.pop();
            }
        }
    }

    pub fn command_preview(&self) -> String {
        let mut parts = vec!["wcl".to_string(), self.input_url.clone()];
        if self.flag_site {
            parts.push("--site".to_string());
        }
        if !self.input_match.trim().is_empty() {
            parts.push(format!("--match \"{}\"", self.input_match.trim()));
        }
        if !self.input_output_dir.trim().is_empty() {
            parts.push(format!("-O {}", self.input_output_dir.trim()));
        }
        if self.input_concurrency.trim() != "16" && !self.input_concurrency.trim().is_empty() {
            parts.push(format!("--concurrency {}", self.input_concurrency.trim()));
        }
        if !self.input_max_pages.trim().is_empty() && self.input_max_pages.trim() != "0" {
            parts.push(format!("--pages {}", self.input_max_pages.trim()));
        }
        if !self.input_query.trim().is_empty() {
            parts.push(format!("-q \"{}\"", self.input_query.trim()));
        }
        if !self.input_css.trim().is_empty() {
            parts.push(format!("--css \"{}\"", self.input_css.trim()));
        }
        if self.flag_prune {
            parts.push("--prune".to_string());
        }
        if self.flag_citations {
            parts.push("--citations".to_string());
        }
        if self.flag_refs {
            parts.push("--refs".to_string());
        }
        if self.flag_raw {
            parts.push("--raw".to_string());
        }
        if self.flag_browser {
            parts.push("--browser".to_string());
        }
        if self.flag_ignore_robots {
            parts.push("--ignore-robots".to_string());
        }
        if self.flag_no_delay {
            parts.push("--no-delay".to_string());
        }
        if self.flag_no_links {
            parts.push("--no-links".to_string());
        }
        if self.flag_no_images {
            parts.push("--no-images".to_string());
        }
        if self.flag_no_metadata {
            parts.push("--no-metadata".to_string());
        }
        parts.join(" ")
    }
}

pub async fn run_tui(opts: Options) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = TuiApp::new(opts);
    let res = run_loop(&mut terminal, &mut app).await;

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    if let Err(e) = res {
        eprintln!("TUI Error: {e}");
    }
    Ok(())
}

async fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut TuiApp,
) -> anyhow::Result<()> {
    let (tx, mut rx) = mpsc::channel::<CrawlEvent>(100);

    let mut crawl_started = false;
    let tick_rate = Duration::from_millis(50);

    loop {
        if app.state == TuiState::Crawling && (!crawl_started || app.needs_restart) {
            crawl_started = true;
            app.needs_restart = false;
            let crawl_opts = app.opts.clone();
            let tx_clone = tx.clone();
            tokio::spawn(async move {
                execute_crawl(crawl_opts, tx_clone).await;
            });
        }

        while let Ok(event) = rx.try_recv() {
            match event {
                CrawlEvent::Started { total } => {
                    app.total_pages = total;
                    app.status_msg = format!("Discovered {total} pages from sitemap");
                }
                CrawlEvent::PageDone(page) => {
                    app.processed_count += 1;
                    app.pages.push(page);
                    if app.list_state.selected().is_none() {
                        app.list_state.select(Some(0));
                    }
                }
                CrawlEvent::Error(err) => {
                    app.processed_count += 1;
                    app.error_count += 1;
                    app.status_msg = format!("Skipped: {err}");
                }
                CrawlEvent::Finished => {
                    app.state = TuiState::Completed;
                    app.completed_duration = Some(app.start_time.elapsed());
                    let out_dest = app
                        .opts
                        .output_dir
                        .as_ref()
                        .map(|p| p.display().to_string())
                        .unwrap_or_else(|| "memory".into());
                    app.status_msg = format!(
                        "✓ Finished! {} pages (~{} tokens) saved | Location: {}",
                        app.pages.len(),
                        app.total_tokens(),
                        out_dest
                    );
                }
            }
        }

        terminal.draw(|f| draw_ui(f, app))?;

        if crossterm::event::poll(tick_rate)? {
            if let Event::Key(key) = crossterm::event::read()? {
                if key.kind == KeyEventKind::Press {
                    match app.state {
                        TuiState::Config => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                            KeyCode::Tab => {
                                app.active_input = match app.active_input {
                                    ActiveInput::Url => ActiveInput::Match,
                                    ActiveInput::Match => ActiveInput::OutputDir,
                                    ActiveInput::OutputDir => ActiveInput::Concurrency,
                                    ActiveInput::Concurrency => ActiveInput::MaxPages,
                                    ActiveInput::MaxPages => ActiveInput::Query,
                                    ActiveInput::Query => ActiveInput::Css,
                                    ActiveInput::Css => ActiveInput::Flags,
                                    ActiveInput::Flags => ActiveInput::Url,
                                };
                            }
                            KeyCode::BackTab => {
                                app.active_input = match app.active_input {
                                    ActiveInput::Url => ActiveInput::Flags,
                                    ActiveInput::Match => ActiveInput::Url,
                                    ActiveInput::OutputDir => ActiveInput::Match,
                                    ActiveInput::Concurrency => ActiveInput::OutputDir,
                                    ActiveInput::MaxPages => ActiveInput::Concurrency,
                                    ActiveInput::Query => ActiveInput::MaxPages,
                                    ActiveInput::Css => ActiveInput::Query,
                                    ActiveInput::Flags => ActiveInput::Css,
                                };
                            }
                            KeyCode::Enter => {
                                app.apply_config();
                            }
                            KeyCode::Up => {
                                app.active_input = ActiveInput::Flags;
                                app.selected_flag_index = app.selected_flag_index.saturating_sub(1);
                            }
                            KeyCode::Down => {
                                app.active_input = ActiveInput::Flags;
                                if app.selected_flag_index < 10 {
                                    app.selected_flag_index += 1;
                                }
                            }
                            KeyCode::Right => {
                                app.active_input = ActiveInput::Flags;
                            }
                            KeyCode::Left => {
                                if app.active_input == ActiveInput::Flags {
                                    app.active_input = ActiveInput::Url;
                                }
                            }
                            KeyCode::Char(' ') if app.active_input == ActiveInput::Flags => {
                                match app.selected_flag_index {
                                    0 => app.flag_site = !app.flag_site,
                                    1 => app.flag_prune = !app.flag_prune,
                                    2 => app.flag_citations = !app.flag_citations,
                                    3 => app.flag_refs = !app.flag_refs,
                                    4 => app.flag_raw = !app.flag_raw,
                                    5 => app.flag_browser = !app.flag_browser,
                                    6 => app.flag_ignore_robots = !app.flag_ignore_robots,
                                    7 => app.flag_no_delay = !app.flag_no_delay,
                                    8 => app.flag_no_links = !app.flag_no_links,
                                    9 => app.flag_no_images = !app.flag_no_images,
                                    10 => app.flag_no_metadata = !app.flag_no_metadata,
                                    _ => {}
                                }
                            }
                            // Clear entire input line: Cmd+Backspace, Cmd+Delete, Ctrl+U, Ctrl+K
                            _ if (key.modifiers.contains(KeyModifiers::SUPER)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && matches!(
                                    key.code,
                                    KeyCode::Backspace
                                        | KeyCode::Delete
                                        | KeyCode::Char('u')
                                        | KeyCode::Char('k')
                                ) =>
                            {
                                app.clear_active_input();
                            }
                            // Delete word back: Option+Backspace, Ctrl+W, Option+Delete
                            _ if (key.modifiers.contains(KeyModifiers::ALT)
                                || key.modifiers.contains(KeyModifiers::CONTROL))
                                && matches!(
                                    key.code,
                                    KeyCode::Backspace | KeyCode::Delete | KeyCode::Char('w')
                                ) =>
                            {
                                app.delete_word_back();
                            }
                            KeyCode::Delete => {
                                app.clear_active_input();
                            }
                            KeyCode::Backspace => {
                                if let Some(s) = app.active_text_mut() {
                                    s.pop();
                                }
                            }
                            KeyCode::Char(c) => match app.active_input {
                                ActiveInput::Url => app.input_url.push(c),
                                ActiveInput::Match => app.input_match.push(c),
                                ActiveInput::OutputDir => app.input_output_dir.push(c),
                                ActiveInput::Concurrency => {
                                    if c.is_ascii_digit() {
                                        app.input_concurrency.push(c);
                                    }
                                }
                                ActiveInput::MaxPages => {
                                    if c.is_ascii_digit() {
                                        app.input_max_pages.push(c);
                                    }
                                }
                                ActiveInput::Query => app.input_query.push(c),
                                ActiveInput::Css => app.input_css.push(c),
                                ActiveInput::Flags => {}
                            },
                            _ => {}
                        },
                        TuiState::Crawling | TuiState::Completed => {
                            if app.is_searching {
                                match key.code {
                                    KeyCode::Esc | KeyCode::Enter => {
                                        app.is_searching = false;
                                    }
                                    _ if (key.modifiers.contains(KeyModifiers::SUPER)
                                        || key.modifiers.contains(KeyModifiers::CONTROL))
                                        && matches!(
                                            key.code,
                                            KeyCode::Backspace
                                                | KeyCode::Delete
                                                | KeyCode::Char('u')
                                                | KeyCode::Char('k')
                                        ) =>
                                    {
                                        app.search_query.clear();
                                        app.list_state.select(Some(0));
                                    }
                                    _ if (key.modifiers.contains(KeyModifiers::ALT)
                                        || key.modifiers.contains(KeyModifiers::CONTROL))
                                        && matches!(
                                            key.code,
                                            KeyCode::Backspace
                                                | KeyCode::Delete
                                                | KeyCode::Char('w')
                                        ) =>
                                    {
                                        while app.search_query.ends_with(char::is_whitespace) {
                                            app.search_query.pop();
                                        }
                                        while let Some(c) = app.search_query.chars().last() {
                                            if c.is_whitespace() || c == '/' || c == ':' || c == '.'
                                            {
                                                app.search_query.pop();
                                                break;
                                            }
                                            app.search_query.pop();
                                        }
                                        app.list_state.select(Some(0));
                                    }
                                    KeyCode::Delete => {
                                        app.search_query.clear();
                                        app.list_state.select(Some(0));
                                    }
                                    KeyCode::Backspace => {
                                        app.search_query.pop();
                                        app.list_state.select(Some(0));
                                    }
                                    KeyCode::Char(c) => {
                                        app.search_query.push(c);
                                        app.list_state.select(Some(0));
                                    }
                                    _ => {}
                                }
                            } else {
                                match key.code {
                                    KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                                    KeyCode::Char('r') => {
                                        // Return to config screen to change URL / settings
                                        app.state = TuiState::Config;
                                        app.active_input = ActiveInput::Url;
                                        app.show_args_modal = false;
                                    }
                                    KeyCode::Char('/') => {
                                        app.is_searching = true;
                                    }
                                    KeyCode::Char('a') | KeyCode::Tab => {
                                        app.show_args_modal = !app.show_args_modal;
                                    }
                                    KeyCode::Up | KeyCode::Char('k') => {
                                        let filtered = app.filtered_indices();
                                        if !filtered.is_empty() {
                                            let i = match app.list_state.selected() {
                                                Some(i) if i > 0 => i - 1,
                                                _ => 0,
                                            };
                                            app.list_state.select(Some(i));
                                            app.preview_scroll = 0;
                                        }
                                    }
                                    KeyCode::Down | KeyCode::Char('j') => {
                                        let filtered = app.filtered_indices();
                                        if !filtered.is_empty() {
                                            let i = match app.list_state.selected() {
                                                Some(i) if i < filtered.len() - 1 => i + 1,
                                                Some(i) => i,
                                                None => 0,
                                            };
                                            app.list_state.select(Some(i));
                                            app.preview_scroll = 0;
                                        }
                                    }
                                    KeyCode::PageUp => {
                                        app.preview_scroll = app.preview_scroll.saturating_sub(10);
                                    }
                                    KeyCode::PageDown => {
                                        app.preview_scroll = app.preview_scroll.saturating_add(10);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn execute_crawl(opts: Options, tx: mpsc::Sender<CrawlEvent>) {
    let client = crate::fetch::http::build_client(&opts).unwrap_or_else(|_| reqwest::Client::new());
    let base = match Url::parse(&opts.url) {
        Ok(u) => u,
        Err(e) => {
            let _ = tx.send(CrawlEvent::Error(e.to_string())).await;
            let _ = tx.send(CrawlEvent::Finished).await;
            return;
        }
    };

    let politeness = crate::fetch::politeness::Politeness::new(
        client.clone(),
        opts.ignore_robots,
        opts.no_delay,
    );

    let urls = if opts.site {
        let seeds = if crate::sitemap::looks_like_sitemap(&opts.url) {
            vec![opts.url.clone()]
        } else {
            crate::sitemap::discover(&politeness, &base).await
        };
        match crate::sitemap::collect_urls(
            &client,
            &base,
            seeds,
            opts.match_pattern.as_deref(),
            opts.page_limit(),
        )
        .await
        {
            Ok(u) => u,
            Err(e) => {
                let _ = tx.send(CrawlEvent::Error(e.to_string())).await;
                let _ = tx.send(CrawlEvent::Finished).await;
                return;
            }
        }
    } else {
        vec![base]
    };

    let total = urls.len();
    let _ = tx.send(CrawlEvent::Started { total }).await;

    let engine = match crate::crawl::Engine::build(&opts).await {
        Ok(e) => e,
        Err(e) => {
            let _ = tx.send(CrawlEvent::Error(e.to_string())).await;
            let _ = tx.send(CrawlEvent::Finished).await;
            return;
        }
    };

    if let Some(dir) = &opts.output_dir {
        let _ = std::fs::create_dir_all(dir);
    }

    use futures::stream::{self, StreamExt};
    let pages: Vec<Page> = stream::iter(urls.into_iter().enumerate())
        .map(|(i, u)| {
            let tx = tx.clone();
            let opts = opts.clone();
            let engine = engine.clone();
            let politeness = politeness.clone();
            async move {
                if let Err(e) = politeness.check(&u).await {
                    let _ = tx.send(CrawlEvent::Error(format!("{u}: {e}"))).await;
                    return None;
                }
                politeness.wait(&u).await;

                let r = match engine.fetch(&u).await {
                    Ok(r) => r,
                    Err(e) => {
                        let _ = tx.send(CrawlEvent::Error(format!("{u}: {e}"))).await;
                        return None;
                    }
                };
                let page = match crate::crawl::pipeline(&r, &opts) {
                    Ok(p) => p,
                    Err(e) => {
                        let _ = tx.send(CrawlEvent::Error(format!("{u}: {e}"))).await;
                        return None;
                    }
                };

                if let Some(ref dir) = opts.output_dir {
                    let filename = output::page_filename(i, total, &page.url);
                    let _ = std::fs::write(dir.join(filename), &page.markdown);
                }

                let _ = tx.send(CrawlEvent::PageDone(page.clone())).await;
                Some(page)
            }
        })
        .buffer_unordered(opts.concurrency)
        .filter_map(|x| async move { x })
        .collect()
        .await;

    if let Some(dir) = &opts.output_dir {
        let index_content = output::build_index(&pages);
        let _ = std::fs::write(dir.join("00_INDEX.md"), &index_content);
        let _ = std::fs::write(
            dir.join("llms.txt"),
            output::build_llms_txt(&pages, Some(&opts.url)),
        );
        let _ = std::fs::write(
            dir.join("FULL_CONTEXT.md"),
            output::build_full_context(&pages),
        );
        let _ = std::fs::write(dir.join("docs.jsonl"), output::build_jsonl(&pages));
    }

    let _ = tx.send(CrawlEvent::Finished).await;
}

fn draw_ui(f: &mut Frame, app: &TuiApp) {
    let size = f.area();

    match app.state {
        TuiState::Config => draw_config_screen(f, app, size),
        TuiState::Crawling | TuiState::Completed => draw_dashboard_screen(f, app, size),
    }
}

fn draw_config_screen(f: &mut Frame, app: &TuiApp, area: Rect) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(14),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(area);

    let title_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(Color::Cyan))
        .title(" ⚡ WCL (Web Crawler LLM) - Interactive Setup & Arguments ");

    let header = Paragraph::new(
        "Configure your crawl arguments and flags below, then press [ENTER] to start",
    )
    .style(Style::default().fg(Color::White))
    .alignment(Alignment::Center)
    .block(title_block);
    f.render_widget(header, chunks[0]);

    // Split main area into Columns: Inputs (Left 55%) vs Flags & Options (Right 45%)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(55), Constraint::Percentage(45)])
        .split(chunks[1]);

    let input_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .split(cols[0]);

    let render_input = |label: &str, val: &str, active: bool| {
        let border_color = if active {
            Color::Green
        } else {
            Color::DarkGray
        };
        let style = if active {
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(Color::White)
        };
        Paragraph::new(val.to_string()).style(style).block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(border_color))
                .title(format!(" {label} ")),
        )
    };

    f.render_widget(
        render_input(
            "1. Target URL",
            &app.input_url,
            app.active_input == ActiveInput::Url,
        ),
        input_chunks[0],
    );
    f.render_widget(
        render_input(
            "2. Match Pattern (--match filter)",
            &app.input_match,
            app.active_input == ActiveInput::Match,
        ),
        input_chunks[1],
    );
    f.render_widget(
        render_input(
            "3. Output Directory (-O / --output-dir)",
            &app.input_output_dir,
            app.active_input == ActiveInput::OutputDir,
        ),
        input_chunks[2],
    );
    f.render_widget(
        render_input(
            "4. Concurrency (--concurrency 1-256)",
            &app.input_concurrency,
            app.active_input == ActiveInput::Concurrency,
        ),
        input_chunks[3],
    );
    f.render_widget(
        render_input(
            "5. Max Pages Limit (--pages, 0 = all)",
            &app.input_max_pages,
            app.active_input == ActiveInput::MaxPages,
        ),
        input_chunks[4],
    );
    f.render_widget(
        render_input(
            "6. Query Filter (-q / --query BM25)",
            &app.input_query,
            app.active_input == ActiveInput::Query,
        ),
        input_chunks[5],
    );
    f.render_widget(
        render_input(
            "7. Scope CSS Selector (--css)",
            &app.input_css,
            app.active_input == ActiveInput::Css,
        ),
        input_chunks[6],
    );

    // Place the text cursor at the end of the active input so users see where keystrokes land.
    if app.active_input != ActiveInput::Flags {
        let (active_chunk, active_text) = match app.active_input {
            ActiveInput::Url => (input_chunks[0], app.input_url.as_str()),
            ActiveInput::Match => (input_chunks[1], app.input_match.as_str()),
            ActiveInput::OutputDir => (input_chunks[2], app.input_output_dir.as_str()),
            ActiveInput::Concurrency => (input_chunks[3], app.input_concurrency.as_str()),
            ActiveInput::MaxPages => (input_chunks[4], app.input_max_pages.as_str()),
            ActiveInput::Query => (input_chunks[5], app.input_query.as_str()),
            ActiveInput::Css => (input_chunks[6], app.input_css.as_str()),
            ActiveInput::Flags => unreachable!(),
        };
        let inner_x = active_chunk.x.saturating_add(1);
        let inner_y = active_chunk.y.saturating_add(1);
        let inner_w = active_chunk.width.saturating_sub(2);
        let max_cursor_x = inner_x.saturating_add(inner_w.saturating_sub(1));
        let cursor_x = inner_x
            .saturating_add(active_text.chars().count() as u16)
            .min(max_cursor_x);
        f.set_cursor_position(ratatui::layout::Position {
            x: cursor_x,
            y: inner_y,
        });
    }

    // Right Column: Toggleable Flag Switches
    let flags_list = vec![
        (
            "Site Mode (--site)",
            app.flag_site,
            "Crawl entire website via sitemap discovery",
        ),
        (
            "Prune Noise (--prune)",
            app.flag_prune,
            "Aggressive boilerplate/nav stripping",
        ),
        (
            "Citations (--citations)",
            app.flag_citations,
            "Convert inline links to [1] footnotes",
        ),
        (
            "References (--refs)",
            app.flag_refs,
            "Append a resolved references section",
        ),
        (
            "Raw HTML (--raw)",
            app.flag_raw,
            "Disable cleaning, convert untouched HTML",
        ),
        (
            "Headless Browser (--browser)",
            app.flag_browser,
            "Use Chromium engine for JS sites",
        ),
        (
            "Ignore Robots (--ignore-robots)",
            app.flag_ignore_robots,
            "Bypass robots.txt disallow rules",
        ),
        (
            "No Crawl-Delay (--no-delay)",
            app.flag_no_delay,
            "Skip rate-limit waits; ignores Crawl-delay in robots.txt",
        ),
        (
            "Strip Links (--no-links)",
            app.flag_no_links,
            "Keep anchor text but drop URL hrefs",
        ),
        (
            "Strip Images (--no-images)",
            app.flag_no_images,
            "Drop markdown image tags",
        ),
        (
            "Omit Metadata (--no-metadata)",
            app.flag_no_metadata,
            "Suppress YAML frontmatter header",
        ),
    ];

    let is_flags_active = app.active_input == ActiveInput::Flags;
    let flag_items: Vec<ListItem> = flags_list
        .iter()
        .enumerate()
        .map(|(i, (name, val, desc))| {
            let is_focused = is_flags_active && app.selected_flag_index == i;
            let is_cursor = app.selected_flag_index == i;
            let check = if *val { "[x]" } else { "[ ]" };
            let check_color = if *val { Color::Green } else { Color::DarkGray };
            let row_style = if is_focused {
                Style::default()
                    .bg(Color::Rgb(30, 60, 100))
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD)
            } else if is_cursor {
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::UNDERLINED)
            } else {
                Style::default().fg(Color::White)
            };

            let prefix = if is_cursor { "▶ " } else { "  " };

            let line = Line::from(vec![
                Span::styled(
                    prefix,
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!("{check} "),
                    Style::default()
                        .fg(check_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(format!("{name:<26} "), row_style),
                Span::styled(format!("- {desc}"), Style::default().fg(Color::DarkGray)),
            ]);
            ListItem::new(line)
        })
        .collect();

    let flags_block = Block::default()
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded)
        .border_style(Style::default().fg(if is_flags_active {
            Color::Green
        } else {
            Color::DarkGray
        }))
        .title(" CLI Flags & Options ([↑/↓] Select, [Space] Toggle) ");

    let flags_widget = List::new(flag_items).block(flags_block);
    f.render_widget(flags_widget, cols[1]);

    // Live Generated CLI Command Preview
    let cmd_preview = Line::from(vec![
        Span::styled(
            "❯ Generated CLI Command: ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(app.command_preview(), Style::default().fg(Color::Yellow)),
    ]);
    let cmd_widget = Paragraph::new(cmd_preview).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray)),
    );
    f.render_widget(cmd_widget, chunks[2]);

    let footer = Paragraph::new("[Tab/Shift+Tab] Cycle Inputs | [↑/↓] Select Flag | [Space] Toggle Flag | [Enter] Start Crawl | [q/Esc] Quit")
        .style(Style::default().fg(Color::DarkGray))
        .alignment(Alignment::Center);
    f.render_widget(footer, chunks[3]);
}

fn draw_dashboard_screen(f: &mut Frame, app: &TuiApp, area: Rect) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Min(10),
            Constraint::Length(3),
        ])
        .split(area);

    let elapsed = app
        .completed_duration
        .unwrap_or_else(|| app.start_time.elapsed());
    let secs = elapsed.as_secs_f64().max(0.001);
    let speed = if app.state == TuiState::Completed {
        (app.pages.len() as f64) / secs
    } else {
        (app.processed_count as f64) / secs
    };

    let status_badge = if app.state == TuiState::Completed {
        Span::styled(
            " [FINISHED] ",
            Style::default()
                .bg(Color::Green)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
    } else {
        Span::styled(
            " [CRAWLING] ",
            Style::default()
                .bg(Color::Yellow)
                .fg(Color::Black)
                .add_modifier(Modifier::BOLD),
        )
    };

    let total_tokens = app.total_tokens();
    let header_text = Line::from(vec![
        Span::styled(
            "⚡ WCL DASHBOARD ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        status_badge,
        Span::raw(" │ Target: "),
        Span::styled(&app.opts.url, Style::default().fg(Color::Yellow)),
        Span::raw(" │ Time: "),
        Span::styled(format!("{:.1}s", secs), Style::default().fg(Color::White)),
        Span::raw(" │ Speed: "),
        Span::styled(
            format!("{:.1} pages/s", speed),
            Style::default()
                .fg(Color::Green)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ Tokens: "),
        Span::styled(
            format!("~{:.1}k", total_tokens as f64 / 1000.0),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(" │ Workers: "),
        Span::styled(
            format!("{}", app.opts.concurrency),
            Style::default().fg(Color::Magenta),
        ),
    ]);

    let header_block = Paragraph::new(header_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if app.state == TuiState::Completed {
                    Color::Green
                } else {
                    Color::Cyan
                })),
        )
        .alignment(Alignment::Left);
    f.render_widget(header_block, main_chunks[0]);

    // Active Crawl Arguments Banner
    let mut flags_summary = Vec::new();
    if app.opts.site {
        flags_summary.push("--site");
    }
    if app.opts.match_pattern.is_some() {
        flags_summary.push("--match");
    }
    if app.opts.prune {
        flags_summary.push("--prune");
    }
    if app.opts.citations {
        flags_summary.push("--citations");
    }
    if app.opts.refs {
        flags_summary.push("--refs");
    }
    if app.opts.raw {
        flags_summary.push("--raw");
    }
    if app.opts.browser {
        flags_summary.push("--browser");
    }
    if app.opts.ignore_robots {
        flags_summary.push("--ignore-robots");
    }
    if app.opts.no_links {
        flags_summary.push("--no-links");
    }
    if app.opts.no_images {
        flags_summary.push("--no-images");
    }
    if app.opts.no_metadata {
        flags_summary.push("--no-metadata");
    }

    let flags_str = if flags_summary.is_empty() {
        "Default".to_string()
    } else {
        flags_summary.join(" ")
    };

    let args_line = Line::from(vec![
        Span::styled(
            "⚙ Active Args: ",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("Output: "),
        Span::styled(
            app.opts
                .output_dir
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "stdout".into()),
            Style::default().fg(Color::Yellow),
        ),
        Span::raw(" │ Filter: "),
        Span::styled(
            app.opts.match_pattern.as_deref().unwrap_or("none"),
            Style::default().fg(Color::White),
        ),
        Span::raw(" │ Flags: "),
        Span::styled(flags_str, Style::default().fg(Color::Green)),
        Span::raw(" │ Full CLI: "),
        Span::styled(
            format!("wcl {}", app.command_preview().replace("wcl ", "")),
            Style::default().fg(Color::DarkGray),
        ),
    ]);
    let args_widget = Paragraph::new(args_line).block(
        Block::default()
            .borders(Borders::ALL)
            .border_type(BorderType::Rounded)
            .border_style(Style::default().fg(Color::DarkGray))
            .title(" Crawl Arguments & Settings ([Tab / a] to Inspect) "),
    );
    f.render_widget(args_widget, main_chunks[1]);

    let progress = if app.state == TuiState::Completed {
        1.0
    } else if app.total_pages > 0 {
        (app.processed_count as f64 / app.total_pages as f64).min(1.0)
    } else {
        0.0
    };

    let gauge_title = if app.state == TuiState::Completed {
        if app.error_count > 0 {
            format!(
                " ✓ Complete ({} saved, {} skipped / {} total) - Press [q] to Exit ",
                app.pages.len(),
                app.error_count,
                app.total_pages
            )
        } else {
            format!(
                " ✓ Complete ({} / {} pages) - Press [q] to Exit ",
                app.pages.len(),
                app.total_pages
            )
        }
    } else {
        format!(
            " Crawling... ({}/{} pages processed) ",
            app.processed_count, app.total_pages
        )
    };

    let gauge = Gauge::default()
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if app.state == TuiState::Completed {
                    Color::Green
                } else {
                    Color::Cyan
                }))
                .title(gauge_title),
        )
        .gauge_style(
            Style::default()
                .fg(if app.state == TuiState::Completed {
                    Color::Green
                } else {
                    Color::Cyan
                })
                .bg(Color::DarkGray),
        )
        .percent((progress * 100.0) as u16);
    f.render_widget(gauge, main_chunks[2]);

    let body_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(40), Constraint::Percentage(60)])
        .split(main_chunks[3]);

    let filtered_indices = app.filtered_indices();
    let items: Vec<ListItem> = filtered_indices
        .iter()
        .map(|&idx| {
            let p = &app.pages[idx];
            let title = p.title.as_deref().unwrap_or("Untitled");
            let line = Line::from(vec![
                Span::styled(
                    format!("[{:03}] ", idx + 1),
                    Style::default().fg(Color::DarkGray),
                ),
                Span::styled("✓ ", Style::default().fg(Color::Green)),
                Span::styled(title, Style::default().fg(Color::White)),
                Span::styled(
                    format!(" (~{} t)", p.tokens),
                    Style::default().fg(Color::DarkGray),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let list_title = if app.is_searching {
        format!(
            " Search (/): [{}] (Press [Esc] to clear) ",
            app.search_query
        )
    } else if !app.search_query.is_empty() {
        format!(
            " Crawled Pages ({}/{}) [Filter: '{}'] ",
            filtered_indices.len(),
            app.pages.len(),
            app.search_query
        )
    } else {
        format!(" Crawled Pages ({}) [/ to search] ", app.pages.len())
    };

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(if app.is_searching {
                    Color::Yellow
                } else {
                    Color::DarkGray
                }))
                .title(list_title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::Rgb(30, 60, 100))
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )
        .highlight_symbol("▶ ");

    f.render_stateful_widget(list, body_chunks[0], &mut app.list_state.clone());

    if app.is_searching {
        let list_area = body_chunks[0];
        // Title bar lives on the top border row. Place cursor at end of "[query]" segment.
        let prefix = " Search (/): [";
        let cursor_x = list_area
            .x
            .saturating_add(1)
            .saturating_add(prefix.chars().count() as u16)
            .saturating_add(app.search_query.chars().count() as u16);
        f.set_cursor_position(ratatui::layout::Position {
            x: cursor_x,
            y: list_area.y,
        });
    }

    let selected_filtered_pos = app.list_state.selected().unwrap_or(0);
    let selected_page_idx = filtered_indices.get(selected_filtered_pos).copied();

    let preview_text = if app.show_args_modal {
        format!(
            "# Active Crawl Arguments & Stats\n\n\
            - Target URL: {}\n\
            - Total Pages: {}\n\
            - Total Tokens: ~{}\n\
            - Site Mode (--site): {}\n\
            - Match Pattern (--match): {:?}\n\
            - Output Directory (-O): {:?}\n\
            - Concurrency (--concurrency): {}\n\
            - Max Pages Limit (--pages): {}\n\
            - Query Filter (-q): {:?}\n\
            - CSS Scope (--css): {:?}\n\
            - Noise Pruning (--prune): {}\n\
            - Numbered Citations (--citations): {}\n\
            - References Section (--refs): {}\n\
            - Raw HTML conversion (--raw): {}\n\
            - Browser Engine (--browser): {}\n\
            - Ignore robots.txt (--ignore-robots): {}\n\
            - Strip Links (--no-links): {}\n\
            - Strip Images (--no-images): {}\n\
            - Suppress Metadata (--no-metadata): {}\n\n\
            ### Files Generated:\n\
            - `00_INDEX.md` (Table of Contents)\n\
            - `FULL_CONTEXT.md` (Single-file concatenated XML)\n\
            - `docs.jsonl` (JSON Lines for Vector DB)\n\
            - `llms.txt` (Standard LLM index)\n\n\
            ### Equivalent Command Line:\n\
            ```sh\n\
            {}\n\
            ```\n\n\
            (Press [Tab] or [a] to return to live page preview)",
            app.opts.url,
            app.pages.len(),
            app.total_tokens(),
            app.opts.site,
            app.opts.match_pattern,
            app.opts.output_dir,
            app.opts.concurrency,
            app.opts.max_pages,
            app.opts.query,
            app.opts.css,
            app.opts.prune,
            app.opts.citations,
            app.opts.refs,
            app.opts.raw,
            app.opts.browser,
            app.opts.ignore_robots,
            app.opts.no_links,
            app.opts.no_images,
            app.opts.no_metadata,
            app.command_preview()
        )
    } else if let Some(real_idx) = selected_page_idx {
        if let Some(selected_page) = app.pages.get(real_idx) {
            selected_page.markdown.clone()
        } else {
            "No page selected".to_string()
        }
    } else if app.pages.is_empty() {
        "Waiting for first crawled page...".to_string()
    } else {
        "No matching pages found for filter".to_string()
    };

    let preview_title = if app.show_args_modal {
        " ⚙ Crawl Arguments Inspector ".to_string()
    } else if let Some(real_idx) = selected_page_idx {
        if let Some(p) = app.pages.get(real_idx) {
            format!(" Live Markdown Preview [~{} tokens] ", p.tokens)
        } else {
            " Live Markdown Preview ".to_string()
        }
    } else {
        " Live Markdown Preview ".to_string()
    };

    let preview = Paragraph::new(preview_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .title(preview_title),
        )
        .wrap(Wrap { trim: false })
        .scroll((app.preview_scroll, 0));

    f.render_widget(preview, body_chunks[1]);

    let footer_text = Line::from(vec![
        Span::styled(&app.status_msg, Style::default().fg(Color::White)),
        Span::raw(" │ "),
        Span::styled(
            if app.is_searching {
                "Type search query │ [Enter/Esc] Done Searching"
            } else if app.state == TuiState::Completed {
                "[↑/↓] Browse Pages │ [/] Search │ [PgUp/PgDn] Scroll │ [a/Tab] Args │ [r] Reconfigure │ [q/Esc] Exit"
            } else {
                "[↑/↓] Select Page │ [/] Search │ [PgUp/PgDn] Scroll │ [a/Tab] Args │ [r] Reconfigure │ [q/Esc] Exit"
            },
            Style::default().fg(Color::DarkGray),
        ),
    ]);

    let footer = Paragraph::new(footer_text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_type(BorderType::Rounded)
                .border_style(Style::default().fg(Color::DarkGray)),
        )
        .alignment(Alignment::Center);
    f.render_widget(footer, main_chunks[4]);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Options;
    use ratatui::backend::{Backend, TestBackend};
    use ratatui::Terminal;

    fn empty_opts() -> Options {
        Options {
            url: String::new(),
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
            timeout_ms: 30_000,
            site: false,
            match_pattern: None,
            deep: None,
            max_pages: 0,
            max_depth: 2,
            concurrency: 16,
            browser: false,
            ignore_robots: false,
            no_delay: false,
            no_metadata: false,
            verbose: false,
            tui: true,
            copy: false,
            jsonl: false,
            output: None,
            output_dir: None,
        }
    }

    fn make_app() -> TuiApp {
        TuiApp::new(empty_opts())
    }

    #[test]
    fn config_screen_renders_cursor_in_active_input() {
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        let app = make_app();
        terminal
            .draw(|f| draw_config_screen(f, &app, f.area()))
            .unwrap();
        let pos = terminal.backend_mut().get_cursor_position().unwrap();
        // URL field is at x=2,y=5 (1px outer margin + 1px border on each side);
        // it starts empty so the cursor sits right at the field's left edge.
        assert_eq!(
            pos,
            ratatui::layout::Position { x: 2, y: 5 },
            "cursor should sit at end of the active URL field; got {pos:?}"
        );
    }

    #[test]
    fn config_screen_cursor_tracks_active_input_change() {
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        let mut app = make_app();
        app.active_input = ActiveInput::OutputDir;
        app.input_output_dir = "/tmp/x".to_string();
        terminal
            .draw(|f| draw_config_screen(f, &app, f.area()))
            .unwrap();
        let pos = terminal.backend_mut().get_cursor_position().unwrap();
        // Output dir is the 3rd input chunk. input_chunks[0] starts at y=4 (top of body area),
        // each chunk is height 3, so [2] starts at y=10; the cursor's inner-row y is 11.
        assert_eq!(
            pos,
            ratatui::layout::Position { x: 2 + 6, y: 11 },
            "cursor should track OutputDir field; got {pos:?}"
        );
    }

    #[test]
    fn config_screen_cursor_not_placed_when_flags_active() {
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        let mut app = make_app();
        app.active_input = ActiveInput::Flags;
        terminal
            .draw(|f| draw_config_screen(f, &app, f.area()))
            .unwrap();
        // Flags mode has no text input — TestBackend default cursor (0,0) is acceptable;
        // the selection cursor is the ▶ glyph inside the list.
        let pos = terminal.backend_mut().get_cursor_position().unwrap();
        assert_eq!(
            pos,
            ratatui::layout::Position { x: 0, y: 0 },
            "flags mode should not set a text cursor; got {pos:?}"
        );
    }

    #[test]
    fn dashboard_search_mode_renders_cursor_in_list_title() {
        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        let mut app = make_app();
        app.state = TuiState::Crawling;
        app.is_searching = true;
        app.search_query = "doc".to_string();
        terminal
            .draw(|f| draw_dashboard_screen(f, &app, f.area()))
            .unwrap();
        let pos = terminal.backend_mut().get_cursor_position().unwrap();
        // Title bar is the top border row of the list block. With 40 rows total the
        // body section starts after header (3) + args (3) + gauge (3) = 9, plus margin.
        // The cursor must sit on that top border row (y in a small range near body start)
        // and beyond the " Search (/): [" prefix.
        assert!(
            pos.y >= 7 && pos.y <= 12,
            "cursor y out of body range, got {pos:?}"
        );
        assert!(
            pos.x > 16,
            "cursor should advance past 'Search (/): [' prefix, got {pos:?}"
        );
    }
}
