use std::time::Instant;
use url::Url;
use wcl::fetch::politeness::Politeness;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn server_with_robots(body: &str) -> MockServer {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/robots.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string(body))
        .mount(&s)
        .await;
    s
}

#[tokio::test]
async fn disallowed_path_is_rejected() {
    let s = server_with_robots("User-agent: *\nDisallow: /private/").await;
    let p = Politeness::new(reqwest::Client::new(), false, false);
    let url = Url::parse(&format!("{}/private/secret", s.uri())).unwrap();
    assert!(p.check(&url).await.is_err());
}

#[tokio::test]
async fn allowed_path_passes() {
    let s = server_with_robots("User-agent: *\nDisallow: /private").await;
    let p = Politeness::new(reqwest::Client::new(), false, false);
    let url = Url::parse(&format!("{}/docs/intro", s.uri())).unwrap();
    assert!(p.check(&url).await.is_ok());
}

#[tokio::test]
async fn ignore_robots_bypasses_disallow() {
    let s = server_with_robots("User-agent: *\nDisallow: /").await;
    let p = Politeness::new(reqwest::Client::new(), true, false);
    let url = Url::parse(&format!("{}/anything", s.uri())).unwrap();
    assert!(p.check(&url).await.is_ok());
}

#[tokio::test]
async fn missing_robots_txt_allows_everything() {
    let s = MockServer::start().await; // no /robots.txt mock -> 404
    let p = Politeness::new(reqwest::Client::new(), false, false);
    let url = Url::parse(&format!("{}/page", s.uri())).unwrap();
    assert!(p.check(&url).await.is_ok());
}

#[tokio::test]
async fn crawl_delay_is_enforced_between_requests_to_same_host() {
    let s = server_with_robots("User-agent: *\nCrawl-delay: 0.05").await;
    let p = Politeness::new(reqwest::Client::new(), false, false);
    let url = Url::parse(&format!("{}/a", s.uri())).unwrap();
    p.check(&url).await.unwrap();
    p.wait(&url).await;
    let t = Instant::now();
    p.wait(&url).await;
    assert!(
        t.elapsed().as_millis() >= 40,
        "wait should respect crawl-delay: {:?}",
        t.elapsed()
    );
}

#[tokio::test]
async fn ignore_robots_bypasses_crawl_delay() {
    let s = server_with_robots("User-agent: *\nCrawl-delay: 1.0").await;
    let p = Politeness::new(reqwest::Client::new(), true, false);
    let url = Url::parse(&format!("{}/a", s.uri())).unwrap();
    p.check(&url).await.unwrap();
    let t = Instant::now();
    p.wait(&url).await;
    p.wait(&url).await;
    assert!(
        t.elapsed().as_millis() < 100,
        "ignore_robots should bypass wait: {:?}",
        t.elapsed()
    );
}

#[tokio::test]
async fn sitemap_directives_are_extracted() {
    let s = server_with_robots(
        "User-agent: *\nSitemap: https://ex.com/sitemap.xml\nSitemap: https://ex.com/news.xml",
    )
    .await;
    let p = Politeness::new(reqwest::Client::new(), false, false);
    let url = Url::parse(&s.uri()).unwrap();
    let maps = p.sitemaps_for(&url).await;
    assert_eq!(maps.len(), 2);
    assert!(maps.contains(&"https://ex.com/sitemap.xml".to_string()));
}

#[tokio::test]
async fn robots_txt_is_fetched_once_per_host() {
    let s = server_with_robots("User-agent: *\nDisallow:").await;
    let p = Politeness::new(reqwest::Client::new(), false, false);
    for i in 0..5 {
        let u = Url::parse(&format!("{}/p{i}", s.uri())).unwrap();
        p.check(&u).await.unwrap();
    }
    let hits = s
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path() == "/robots.txt")
        .count();
    assert_eq!(hits, 1, "robots.txt refetched {hits} times");
}
