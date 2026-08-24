use url::Url;
use wcl::sitemap::{collect_urls, looks_like_sitemap, parse_xml};
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const URLSET: &str = r#"<?xml version="1.0"?>
<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <url><loc>https://ex.com/docs/a</loc></url>
  <url><loc>https://ex.com/blog/b</loc></url>
</urlset>"#;

const INDEX: &str = r#"<?xml version="1.0"?>
<sitemapindex xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">
  <sitemap><loc>https://ex.com/sitemap-docs.xml</loc></sitemap>
</sitemapindex>"#;

#[test]
fn parses_a_urlset() {
    let (urls, subs) = parse_xml(URLSET.as_bytes());
    assert_eq!(urls.len(), 2);
    assert!(subs.is_empty());
}

#[test]
fn parses_a_sitemapindex() {
    let (urls, subs) = parse_xml(INDEX.as_bytes());
    assert!(urls.is_empty());
    assert_eq!(subs, vec!["https://ex.com/sitemap-docs.xml"]);
}

#[test]
fn parses_namespaceless_sitemaps() {
    let xml = "<urlset><url><loc>https://ex.com/x</loc></url></urlset>";
    let (urls, _) = parse_xml(xml.as_bytes());
    assert_eq!(urls, vec!["https://ex.com/x"]);
}

#[test]
fn parses_malformed_xml_by_falling_back_to_loc_scanning() {
    let xml = "<urlset><url><loc>https://ex.com/x</loc></urlset>"; // unclosed <url>
    let (urls, _) = parse_xml(xml.as_bytes());
    assert_eq!(urls, vec!["https://ex.com/x"]);
}

#[test]
fn looks_like_sitemap_detects_xml_and_the_word_sitemap() {
    assert!(looks_like_sitemap("https://ex.com/sitemap.xml"));
    assert!(looks_like_sitemap("https://ex.com/SITEMAP_index.xml"));
    assert!(!looks_like_sitemap("https://ex.com/docs/intro"));
}

#[tokio::test]
async fn follows_sitemap_indexes_recursively() {
    let s = MockServer::start().await;
    let idx = format!(
        r#"<sitemapindex><sitemap><loc>{}/sitemap-docs.xml</loc></sitemap></sitemapindex>"#,
        s.uri()
    );
    let leaf = format!(
        r#"<urlset><url><loc>{}/docs/a</loc></url><url><loc>{}/docs/b</loc></url></urlset>"#,
        s.uri(),
        s.uri()
    );
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(idx))
        .mount(&s)
        .await;
    Mock::given(method("GET"))
        .and(path("/sitemap-docs.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(leaf))
        .mount(&s)
        .await;

    let base = Url::parse(&s.uri()).unwrap();
    let seeds = vec![format!("{}/sitemap.xml", s.uri())];
    let urls = collect_urls(&reqwest::Client::new(), &base, seeds, None, None)
        .await
        .unwrap();
    assert_eq!(urls.len(), 2);
}

#[tokio::test]
async fn match_pattern_filters_urls() {
    let s = MockServer::start().await;
    let leaf = format!(
        r#"<urlset><url><loc>{}/docs/a</loc></url><url><loc>{}/blog/b</loc></url></urlset>"#,
        s.uri(),
        s.uri()
    );
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(ResponseTemplate::new(200).set_body_string(leaf))
        .mount(&s)
        .await;

    let base = Url::parse(&s.uri()).unwrap();
    let seeds = vec![format!("{}/sitemap.xml", s.uri())];
    let urls = collect_urls(&reqwest::Client::new(), &base, seeds, Some("/docs/"), None)
        .await
        .unwrap();
    assert_eq!(urls.len(), 1);
    assert!(urls[0].path().starts_with("/docs/"));
}

#[tokio::test]
async fn limit_stops_collection_early() {
    let s = MockServer::start().await;
    let locs: String = (0..50)
        .map(|i| format!("<url><loc>{}/p{i}</loc></url>", s.uri()))
        .collect();
    Mock::given(method("GET"))
        .and(path("/sitemap.xml"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string(format!("<urlset>{locs}</urlset>")),
        )
        .mount(&s)
        .await;

    let base = Url::parse(&s.uri()).unwrap();
    let seeds = vec![format!("{}/sitemap.xml", s.uri())];
    let urls = collect_urls(&reqwest::Client::new(), &base, seeds, None, Some(5))
        .await
        .unwrap();
    assert_eq!(urls.len(), 5);
}

#[test]
fn offsite_urls_are_dropped_but_subdomains_are_kept() {
    let xml = r#"<urlset>
      <url><loc>https://ex.com/a</loc></url>
      <url><loc>https://docs.ex.com/b</loc></url>
      <url><loc>https://evil.test/c</loc></url>
    </urlset>"#;
    let (urls, _) = parse_xml(xml.as_bytes());
    let base = Url::parse("https://ex.com/").unwrap();
    let kept = wcl::sitemap::filter_same_site(&urls, &base, None);
    assert_eq!(kept.len(), 2);
}
