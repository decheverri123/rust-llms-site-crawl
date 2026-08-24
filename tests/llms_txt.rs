use url::Url;
use wcl::llms_txt::probe;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
async fn finds_llms_full_txt() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/llms-full.txt"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string("# Docs\n\nEverything, already in markdown."),
        )
        .mount(&s)
        .await;

    let got = probe(&reqwest::Client::new(), &Url::parse(&s.uri()).unwrap()).await;
    assert!(got.unwrap().contains("already in markdown"));
}

#[tokio::test]
async fn returns_none_when_absent() {
    let s = MockServer::start().await;
    assert!(
        probe(&reqwest::Client::new(), &Url::parse(&s.uri()).unwrap())
            .await
            .is_none()
    );
}

#[tokio::test]
async fn rejects_an_html_error_page_served_with_200() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/llms-full.txt"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                "<!doctype html><html><body>404</body></html>"
                    .as_bytes()
                    .to_vec(),
                "text/html",
            ),
        )
        .mount(&s)
        .await;

    assert!(
        probe(&reqwest::Client::new(), &Url::parse(&s.uri()).unwrap())
            .await
            .is_none()
    );
}

#[tokio::test]
async fn rejects_a_suspiciously_tiny_body() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/llms-full.txt"))
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&s)
        .await;

    assert!(
        probe(&reqwest::Client::new(), &Url::parse(&s.uri()).unwrap())
            .await
            .is_none()
    );
}
