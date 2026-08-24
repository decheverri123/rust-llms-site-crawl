use wcl::markdown::Page;
use wcl::options::Options;
use wcl::output::{build_index, render, slug_for};

fn page(url: &str, title: &str) -> Page {
    Page {
        url: url.into(),
        title: Some(title.into()),
        markdown: format!("# {title}\n\nbody\n"),
        tokens: 100,
        links: vec![],
    }
}

#[test]
fn slug_encodes_the_path_not_just_the_last_segment() {
    // Two pages named "index" in different sections must not collide.
    let a = slug_for("https://ex.com/docs/guides/index");
    let b = slug_for("https://ex.com/docs/api/index");
    assert_ne!(a, b);
}

#[test]
fn slug_handles_root_and_trailing_slash() {
    assert_eq!(slug_for("https://ex.com/"), "index");
    assert_eq!(slug_for("https://ex.com/docs/"), "docs");
}

#[test]
fn slug_strips_characters_illegal_in_filenames() {
    let s = slug_for("https://ex.com/a?b=c&d=e#frag");
    assert!(!s.contains('?') && !s.contains('&') && !s.contains('#'));
}

#[test]
fn index_lists_every_page_with_a_relative_link() {
    let pages = vec![
        page("https://ex.com/docs/a", "A"),
        page("https://ex.com/docs/b", "B"),
    ];
    let idx = build_index(&pages);
    assert!(idx.contains("[A](01_docs-a.md)"), "got: {idx}");
    assert!(idx.contains("[B](02_docs-b.md)"));
}

#[test]
fn output_dir_writes_one_file_per_page_plus_index() {
    let dir = tempfile::tempdir().unwrap();
    let mut o = Options::for_test("https://ex.com/");
    o.output_dir = Some(dir.path().to_path_buf());
    render(&[page("https://ex.com/docs/a", "A")], &o).unwrap();

    assert!(dir.path().join("01_docs-a.md").exists());
    assert!(dir.path().join("00_INDEX.md").exists());
    assert!(dir.path().join("llms.txt").exists());
    assert!(dir.path().join("FULL_CONTEXT.md").exists());
    assert!(dir.path().join("docs.jsonl").exists());
}

#[test]
fn single_file_output_concatenates_pages_with_separators() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("all.md");
    let mut o = Options::for_test("https://ex.com/");
    o.output = Some(path.clone());
    render(
        &[page("https://ex.com/a", "A"), page("https://ex.com/b", "B")],
        &o,
    )
    .unwrap();

    let s = std::fs::read_to_string(&path).unwrap();
    assert!(s.contains("# A") && s.contains("# B"));
    assert_eq!(s.matches("\n---\n").count(), 1, "expected one separator");
}
