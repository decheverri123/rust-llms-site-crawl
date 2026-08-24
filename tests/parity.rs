use insta::assert_snapshot;
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
        html: html.into(),
        content_type: Some("text/html".into()),
    }
}

const DOCUSAURUS: &str = include_str!("fixtures/docusaurus.html");

#[test]
fn golden_default_pipeline() {
    let p = pipeline(&raw(DOCUSAURUS), &Options::for_test("https://example.com/")).unwrap();
    assert_snapshot!(p.markdown);
}

#[test]
fn golden_prune_pipeline() {
    let mut o = Options::for_test("https://example.com/");
    o.prune = true;
    assert_snapshot!(pipeline(&raw(DOCUSAURUS), &o).unwrap().markdown);
}

#[test]
fn golden_citations_pipeline() {
    let mut o = Options::for_test("https://example.com/");
    o.citations = true;
    o.refs = true;
    o.raw = true; // keep nav links so there is something to cite
    assert_snapshot!(pipeline(&raw(DOCUSAURUS), &o).unwrap().markdown);
}
