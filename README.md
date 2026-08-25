# rust-llms-site-crawl (`wcl`)

[![CI](https://github.com/decheverri123/rust-llms-site-crawl/actions/workflows/ci.yml/badge.svg)](https://github.com/decheverri123/rust-llms-site-crawl/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

A zero-config, single-binary CLI that turns **any website into LLM-ready markdown** with a single command. Built in Rust with a fast HTTP-first engine and optional headless Chromium fallback.

```sh
wcl https://example.dev/
```

That's it. You define a website, you immediately get clean markdown on stdout — ready to paste straight into a language model, a RAG pipeline, or a file.

## Why

Raw web HTML is noisy: navigation bars, cookie banners, footers, scripts. Feeding that to an LLM wastes tokens and hurts accuracy. `wcl` provides clean, opinionated defaults:

- **Chrome stripped** — `<nav>`, `<footer>`, `<aside>`, `<form>`, `<script>`, `<style>` removed before markdown.
- **Complete content** — no aggressive pruning by default, so link text and document structure survive.
- **Clean Markdown** — proper headings, tables, code blocks, inline links.
- **Metadata header** — source URL, document title, and crawl timestamp.
- **Sitemap discovery & recursive walk** — effortlessly crawl entire documentation suites.
- **`llms.txt` / `llms-full.txt` fast path** — instantly reuses site-published LLM markdown when available.
- **Polite by default** — obeys `robots.txt` and `Crawl-delay`.
- **Fast HTTP-first architecture** — executes in milliseconds without spawning heavy browsers unless requested.

---

## Quick Start: How to Run

### 1. Install with curl (recommended)

Downloads the latest prebuilt binary for your platform (Linux/macOS, x86_64/arm64) into `~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/decheverri123/rust-llms-site-crawl/main/install.sh | sh
```

Set `WCL_INSTALL_DIR` to install somewhere other than `~/.local/bin`. No prebuilt binary for your platform? Fall back to one of the Cargo-based methods below.

### 2. Run directly with Cargo (No install needed)

You can run `wcl` immediately from the repository using `cargo run`:

```sh
# Crawl a page and output clean markdown to stdout
cargo run -- https://www.example.dev/docs/intro/

# Save to a file
cargo run -- https://www.example.dev/docs/intro/ -o intro.md

# Interactive TUI Mode (Live Terminal UI Dashboard)
cargo run --release -- --tui

# Scrape an entire documentation section via sitemap discovery
cargo run --release -- https://www.example.dev/ --site --match "/docs/" -O ./docs/
```

### Interactive TUI Dashboard (`--tui`)

`wcl` comes with a full **Terminal User Interface (TUI)** built with `ratatui`:

```sh
# Launch interactive config & live dashboard
wcl --tui

# Or simply run without arguments
wcl
```

- **Interactive Configuration**: Type target URLs, match filters, output directories, and concurrency settings.
- **Live Crawling Stream**: Real-time progress bar, page speed stats, and HTTP status codes.
- **Live Markdown Preview**: Split-pane view with syntax highlighting and scrollable preview of any crawled page.

### 3. Build and Run the Release Binary

Compile an optimized, standalone binary:

```sh
# Build the optimized release binary
cargo build --release

# Run the binary directly
./target/release/wcl https://www.example.dev/
```

### 4. Install Globally via Cargo (`wcl` in your `$PATH`)

Install the binary onto your system so you can run `wcl` from any directory:

```sh
# From the local directory
cargo install --path .

# Or install directly from GitHub
cargo install --git https://github.com/decheverri123/rust-llms-site-crawl

# Run from anywhere!
wcl https://news.ycombinator.com
```

### 5. Optional: Enable Headless Browser Support (For JS-heavy sites)

If you need to execute JavaScript, wait for dynamic elements, or scroll infinite pages:

```sh
# Install with the optional browser engine (requires local Chrome/Chromium)
cargo install --path . --features browser

# Or run with cargo:
cargo run --features browser -- https://example.com --browser --wait-for ".loaded"
```

---

## Common Usage Examples

```sh
# 📄 Single page to stdout
wcl https://www.example.dev/

# 💾 Save to a single markdown file
wcl https://www.example.dev/ -o example.md

# 🌐 Whole-site documentation scrape (sitemap discovery -> separate files + INDEX.md)
wcl https://www.example.dev/ --site --match "/docs/" -O ./example_docs/

# 🔍 Query-focused extraction (BM25 keeps top 40% most relevant blocks)
wcl https://www.example.dev/ -q "red teaming"

# 📖 Generate numbered citations and a references section
wcl https://www.example.dev/ --citations --refs

# 🧹 Aggressive readability extraction (prunes boilerplate)
wcl https://www.example.dev/ --prune

# 🎯 Scope extraction to a specific CSS selector
wcl https://www.example.dev/ --css "main article"

# 🕷️ Deep crawl linked pages (breadth-first search, up to 10 pages)
wcl https://docs.rs/tokio/latest/tokio/ --deep bfs --pages 10

# 🤖 Pipe directly to an LLM CLI or tool
wcl https://www.example.dev/ | llm "Summarize the key capabilities"
```

---

## Output Shape

`wcl https://www.example.dev/` produces:

```markdown
<!--
source: https://www.example.dev/
title: Build Secure AI Applications | Promptfoo
crawled_at: 1787595558
-->

# Ship agents, not vulnerabilities

Automated testing that finds & fixes AI risk in development
…
```

Status and progress bars go to **stderr**, so **stdout is always pure markdown** and pipes cleanly:

```sh
wcl https://example.com | llm "summarize this"
wcl https://example.com > page.md
```

---

## Options & Flags Reference

| Flag                   | Description                                                                 |
| ---------------------- | --------------------------------------------------------------------------- |
| `--tui`                | Launch the interactive TUI dashboard (default when no URL is given).        |
| `-o, --output FILE`    | Save output to a single markdown file instead of stdout.                    |
| `-O, --output-dir DIR` | Save each crawled page to separate `.md` files in a directory + `INDEX.md`. |
| `--site`               | Scrape the entire website using sitemap discovery and concurrent crawling.  |
| `--match TEXT`         | Filter whole-site URLs to those containing `TEXT` (e.g. `/docs/`).          |
| `--concurrency N`      | Max concurrent requests (default 16, clamped to 256).                       |
| `--raw`                | The whole page untouched — no chrome stripping, no filtering.               |
| `--prune`              | Aggressive readability extraction (keeps main content).                     |
| `--citations`          | Full content with inline links as numbered citations.                       |
| `--refs`               | Append a references section (implies `--citations`).                        |
| `-q, --query TEXT`     | BM25 query-focused content extraction.                                      |
| `--css SELECTOR`       | Scope extraction to a CSS selector.                                         |
| `--deep bfs\|dfs`      | Deep-crawl linked pages.                                                    |
| `--pages N`            | Max pages for deep crawl / whole-site crawl.                                |
| `--max-depth N`        | Max link depth for deep crawl (default 2).                                  |
| `--no-links`           | Strip all hyperlinks.                                                       |
| `--no-images`          | Strip image references.                                                     |
| `--no-metadata`        | Omit the source/title/timestamp metadata header.                            |
| `--browser`            | Use headless Chrome instead of plain HTTP (requires `--features browser`).  |
| `--wait-for SELECTOR`  | CSS selector to wait for before extracting (implies `--browser`).           |
| `--full`               | Scroll full page for lazy-loaded content (implies `--browser`).             |
| `--ignore-robots`      | Ignore `robots.txt` rules (prints a warning to stderr).                     |
| `--no-delay`           | Skip `Crawl-delay` waits from robots.txt (allow/disallow still applies).    |
| `--timeout MS`         | Page timeout in ms (default 30000).                                         |
| `-c, --copy`           | Copy output markdown directly to the clipboard.                             |
| `--jsonl`              | Export crawled pages to JSON Lines (`.jsonl`).                              |
| `-v, --verbose`        | Verbose logs.                                                               |

---

## License

MIT — see [LICENSE](LICENSE)
