use url::Url;
use wcl::fetch::{Fetcher, HttpFetcher};
use wcl::options::Options;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn test_opts(url: &str) -> Options {
    wcl::options::Options::for_test(url)
}

#[tokio::test]
async fn decodes_utf8_body() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(
            "<h1>café</h1>".as_bytes().to_vec(),
            "text/html; charset=utf-8",
        ))
        .mount(&server)
        .await;

    let f = HttpFetcher::new(&test_opts(&server.uri())).unwrap();
    let page = f.fetch(&Url::parse(&server.uri()).unwrap()).await.unwrap();
    assert!(page.html.contains("café"));
}

#[tokio::test]
async fn decodes_latin1_declared_in_header() {
    let server = MockServer::start().await;
    // 0xE9 is 'é' in ISO-8859-1 but invalid standalone UTF-8.
    let body = b"<h1>caf\xE9</h1>".to_vec();
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(body, "text/html; charset=iso-8859-1"),
        )
        .mount(&server)
        .await;

    let f = HttpFetcher::new(&test_opts(&server.uri())).unwrap();
    let page = f.fetch(&Url::parse(&server.uri()).unwrap()).await.unwrap();
    assert!(page.html.contains("café"), "got: {}", page.html);
}

#[tokio::test]
async fn decodes_charset_from_meta_tag_when_header_is_silent() {
    let server = MockServer::start().await;
    let body = b"<meta charset=\"iso-8859-1\"><h1>caf\xE9</h1>".to_vec();
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_raw(body, "text/html"))
        .mount(&server)
        .await;

    let f = HttpFetcher::new(&test_opts(&server.uri())).unwrap();
    let page = f.fetch(&Url::parse(&server.uri()).unwrap()).await.unwrap();
    assert!(page.html.contains("café"), "got: {}", page.html);
}

#[tokio::test]
async fn records_final_url_after_redirect() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/old"))
        .respond_with(ResponseTemplate::new(301).insert_header("location", "/new"))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/new"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<h1>hi</h1>"))
        .mount(&server)
        .await;

    let f = HttpFetcher::new(&test_opts(&server.uri())).unwrap();
    let page = f
        .fetch(&Url::parse(&format!("{}/old", server.uri())).unwrap())
        .await
        .unwrap();
    assert_eq!(page.final_url.path(), "/new");
}

#[tokio::test]
async fn retries_on_503_then_succeeds() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(503))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_string("<h1>ok</h1>"))
        .mount(&server)
        .await;

    let f = HttpFetcher::new(&test_opts(&server.uri())).unwrap();
    let page = f.fetch(&Url::parse(&server.uri()).unwrap()).await.unwrap();
    assert_eq!(page.status, 200);
}
