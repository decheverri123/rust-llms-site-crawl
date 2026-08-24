use url::Url;
use wcl::crawl::pipeline;
use wcl::fetch::RawPage;
use wcl::options::Options;

fn raw(html: &str) -> RawPage {
    let u = Url::parse("https://example.com/docs/intro").unwrap();
    RawPage {
        requested_url: u.clone(),
        final_url: u,
        status: 200,
        html: html.to_string(),
        content_type: Some("text/html".into()),
    }
}

const FIXTURE: &str = include_str!("fixtures/docusaurus.html");

#[test]
fn default_pipeline_strips_chrome_and_produces_markdown() {
    let p = pipeline(&raw(FIXTURE), &Options::for_test("https://example.com/")).unwrap();
    assert!(p.markdown.contains("Getting Started"));
    assert!(!p.markdown.contains("Blog"));
}

#[test]
fn raw_flag_skips_all_cleaning() {
    let mut o = Options::for_test("https://example.com/");
    o.raw = true;
    let p = pipeline(&raw(FIXTURE), &o).unwrap();
    assert!(p.markdown.contains("Blog"), "chrome should survive --raw");
}

#[test]
fn css_flag_scopes_before_conversion() {
    let mut o = Options::for_test("https://example.com/");
    o.css = Some("main article".into());
    let p = pipeline(&raw(FIXTURE), &o).unwrap();
    assert!(p.markdown.contains("npm i thing"));
    assert!(!p.markdown.contains("Getting Started"));
}

#[test]
fn pipeline_records_the_title() {
    let p = pipeline(&raw(FIXTURE), &Options::for_test("https://example.com/")).unwrap();
    assert_eq!(p.title.as_deref(), Some("Intro"));
}

#[test]
fn pipeline_collects_outbound_links_for_deep_crawl() {
    let p = pipeline(&raw(FIXTURE), &Options::for_test("https://example.com/")).unwrap();
    // Links live in nav/aside, which the default pipeline strips — deep crawl
    // must therefore harvest links from the *unstripped* html.
    assert!(
        p.links.iter().any(|l| l.ends_with("/docs")),
        "links: {:?}",
        p.links
    );
}

#[test]
fn empty_page_is_an_error_not_an_empty_string() {
    let r = pipeline(
        &raw("<html><body><nav>x</nav></body></html>"),
        &Options::for_test("https://example.com/"),
    );
    assert!(r.is_err());
}
