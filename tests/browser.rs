#![cfg(feature = "browser")]

use url::Url;
use wcl::fetch::{browser::BrowserFetcher, Fetcher};
use wcl::options::Options;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

#[tokio::test]
#[ignore = "requires a local Chrome/Chromium installation"]
async fn renders_javascript_injected_content() {
    let s = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/"))
        .respond_with(
            ResponseTemplate::new(200).set_body_raw(
                r#"<html><body><div id="app"></div>
               <script>document.getElementById('app').textContent='rendered by js';</script>
               </body></html>"#
                    .as_bytes()
                    .to_vec(),
                "text/html",
            ),
        )
        .mount(&s)
        .await;

    let f = BrowserFetcher::new(&Options::for_test(&s.uri()))
        .await
        .unwrap();
    let page = f.fetch(&Url::parse(&s.uri()).unwrap()).await.unwrap();
    assert!(page.html.contains("rendered by js"), "JS did not execute");
}
