use wcl::markdown::{convert, extract_title, with_metadata};
use wcl::options::Options;

fn opts() -> Options {
    Options::for_test("https://example.com/")
}
#[test]
fn converts_headings_lists_and_code() {
    let html = "<h1>Title</h1><ul><li>one</li><li>two</li></ul><pre><code>let x = 1;</code></pre>";
    let md = convert(html, &opts()).unwrap();
    assert!(md.contains("# Title"));
    assert!(md.contains("- one"));
    assert!(md.contains("let x = 1;"));
}

#[test]
fn converts_tables_to_pipe_syntax() {
    let html =
        "<table><tr><th>Flag</th><th>Meaning</th></tr><tr><td>-o</td><td>output</td></tr></table>";
    let md = convert(html, &opts()).unwrap();
    assert!(md.contains("| Flag"), "table not converted: {md}");
    assert!(md.contains("| -o"));
}

#[test]
fn keeps_inline_links_by_default() {
    let md = convert(r#"<p>see <a href="/docs">the docs</a></p>"#, &opts()).unwrap();
    assert!(md.contains("[the docs](/docs)"));
}

#[test]
fn no_links_flag_keeps_text_and_drops_targets() {
    let mut o = opts();
    o.no_links = true;
    let md = convert(r#"<p>see <a href="/docs">the docs</a></p>"#, &o).unwrap();
    assert!(md.contains("the docs"));
    assert!(!md.contains("(/docs)"));
}

#[test]
fn no_images_flag_drops_images_entirely() {
    let mut o = opts();
    o.no_images = true;
    let md = convert(r#"<p>a<img src="/x.png" alt="pic">b</p>"#, &o).unwrap();
    assert!(!md.contains("x.png"));
    assert!(!md.contains("pic"));
}

#[test]
fn extract_title_reads_the_title_tag() {
    assert_eq!(
        extract_title("<html><head><title> Intro </title></head></html>").as_deref(),
        Some("Intro")
    );
}

#[test]
fn extract_title_falls_back_to_first_h1() {
    assert_eq!(
        extract_title("<html><body><h1>Fallback</h1></body></html>").as_deref(),
        Some("Fallback")
    );
}

#[test]
fn metadata_header_is_suppressed_by_no_metadata() {
    let o = opts(); // for_test sets no_metadata = true
    let out = with_metadata("body", "https://example.com/", Some("T"), &o);
    assert_eq!(out, "body");
}

#[test]
fn metadata_header_carries_source_and_title() {
    let mut o = opts();
    o.no_metadata = false;
    let out = with_metadata("body", "https://example.com/", Some("T"), &o);
    assert!(out.starts_with("---"));
    assert!(out.contains("source: https://example.com/"));
    assert!(out.contains("title: \"T\""));
    assert!(out.ends_with("body"));
}

use url::Url;
use wcl::markdown::apply_citations;

fn base() -> Url {
    Url::parse("https://example.com/docs/intro").unwrap()
}

#[test]
fn inline_links_become_numbered_citations() {
    let md = "See [the docs](/docs) and [the blog](/blog).";
    let out = apply_citations(md, &base(), false);
    assert!(out.contains("the docs[1]"), "got: {out}");
    assert!(out.contains("the blog[2]"), "got: {out}");
    assert!(!out.contains("](/docs)"));
}

#[test]
fn repeated_targets_reuse_the_same_number() {
    let md = "[a](/x) then [b](/x) then [c](/y)";
    let out = apply_citations(md, &base(), false);
    assert!(out.contains("a[1]") && out.contains("b[1]") && out.contains("c[2]"));
}

#[test]
fn refs_appends_a_resolved_reference_section() {
    let md = "See [the docs](/docs).";
    let out = apply_citations(md, &base(), true);
    assert!(out.contains("## References"));
    assert!(
        out.contains("[1]: https://example.com/docs"),
        "targets must be absolute: {out}"
    );
}

#[test]
fn images_are_not_turned_into_citations() {
    let md = "![alt](/pic.png) and [link](/x)";
    let out = apply_citations(md, &base(), false);
    assert!(
        out.contains("![alt](/pic.png)"),
        "image was rewritten: {out}"
    );
    assert!(out.contains("link[1]"));
}

#[test]
fn code_spans_containing_bracket_syntax_are_left_alone() {
    let md = "use `arr[0](x)` here";
    let out = apply_citations(md, &base(), false);
    assert!(
        out.contains("`arr[0](x)`"),
        "code span was rewritten: {out}"
    );
}

#[test]
fn untagged_rust_block_gets_tagged() {
    let html = "<pre><code>fn main() {\n    let mut x = 1;\n    println!(&quot;{}&quot;, x);\n}</code></pre>";
    let md = convert(html, &opts()).unwrap();
    assert!(md.contains("```rust\n"), "expected rust tag, got:\n{md}");
}

#[test]
fn untagged_python_block_gets_tagged() {
    let html = "<pre><code>def hello(name):\n    print(f&quot;hi {name}&quot;)\n    return name\n</code></pre>";
    let md = convert(html, &opts()).unwrap();
    assert!(
        md.contains("```python\n"),
        "expected python tag, got:\n{md}"
    );
}

#[test]
fn tagged_block_is_not_overwritten() {
    let html = "<pre><code class=\"language-bash\">echo hi</code></pre>";
    let md = convert(html, &opts()).unwrap();
    assert!(md.contains("```bash\n"), "tag lost: {md}");
}

#[test]
fn ambiguous_block_stays_untagged() {
    let html = "<pre><code>hello world</code></pre>";
    let md = convert(html, &opts()).unwrap();
    // Should NOT gain a language tag — no heuristic hit above threshold.
    assert!(!md.contains("```rust\n"), "rust false-positive: {md}");
    assert!(!md.contains("```python\n"), "python false-positive: {md}");
}
