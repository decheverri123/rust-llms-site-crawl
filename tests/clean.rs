use wcl::clean::{prune, scope_to_selector, strip_chrome};

const FIXTURE: &str = include_str!("fixtures/docusaurus.html");

#[test]
fn removes_chrome_tags_and_their_contents() {
    let out = strip_chrome(FIXTURE).unwrap();
    for gone in [
        "navbar",
        "sidebar",
        "tracking",
        "© 2026",
        "enable js",
        "color:red",
    ] {
        assert!(!out.contains(gone), "chrome survived: {gone}");
    }
}

#[test]
fn preserves_header_because_docusaurus_puts_the_hero_there() {
    let out = strip_chrome(FIXTURE).unwrap();
    assert!(out.contains("Getting Started"), "header hero was stripped");
}

#[test]
fn preserves_main_content_including_tables_and_code() {
    let out = strip_chrome(FIXTURE).unwrap();
    assert!(out.contains("npm i thing"));
    assert!(out.contains("<table"));
}

#[test]
fn scope_to_selector_keeps_only_matching_subtree() {
    let out = scope_to_selector(FIXTURE, "main article").unwrap();
    assert!(out.contains("npm i thing"));
    assert!(
        !out.contains("Getting Started"),
        "header is outside main article"
    );
    assert!(!out.contains("navbar"));
}

#[test]
fn scope_to_selector_concatenates_multiple_matches() {
    let html = "<div class=s>one</div><p>skip</p><div class=s>two</div>";
    let out = scope_to_selector(html, "div.s").unwrap();
    assert!(out.contains("one") && out.contains("two"));
    assert!(!out.contains("skip"));
}

#[test]
fn scope_to_selector_errors_on_invalid_selector() {
    assert!(scope_to_selector("<p>x</p>", ">>>bad").is_err());
}

#[test]
fn strip_chrome_is_idempotent() {
    let once = strip_chrome(FIXTURE).unwrap();
    let twice = strip_chrome(&once).unwrap();
    assert_eq!(once, twice);
}

#[test]
fn prune_extracts_article_body_and_drops_boilerplate() {
    let out = prune(FIXTURE, "https://example.com/docs/intro").unwrap();
    assert!(out.html.contains("npm i thing"));
    assert!(!out.html.contains("navbar"));
}

#[test]
fn prune_recovers_the_title() {
    let out = prune(FIXTURE, "https://example.com/docs/intro").unwrap();
    assert_eq!(out.title.as_deref(), Some("Intro"));
}

#[test]
fn prune_preserves_link_text() {
    // Regression guard: some readability extractors drop anchor text as a
    // side effect of content filtering. We explicitly preserve link text.
    let html = r#"<html><head><title>T</title></head><body><article>
        <p>Read the <a href="/guide">setup guide</a> before starting. This paragraph
        exists to clear the readability word-count threshold, which discards short
        blocks as probable boilerplate rather than article prose.</p>
        </article></body></html>"#;
    let out = prune(html, "https://example.com/").unwrap();
    assert!(
        out.html.contains("setup guide"),
        "link text was dropped: {}",
        out.html
    );
}

#[test]
fn prune_never_returns_empty_content() {
    // dom_smoothie 0.18 falls back to the whole body rather than erroring on a
    // chrome-only page. The invariant that matters: prune never silently yields
    // an empty document.
    let html = "<html><body><nav>only chrome</nav></body></html>";
    let out = prune(html, "https://example.com/").unwrap();
    assert!(!out.html.trim().is_empty());
}
