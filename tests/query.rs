use wcl::query::filter_blocks;

const DOC: &str = "\
# Red teaming

Red teaming probes a model for harmful outputs by generating adversarial prompts.

## Installation

Run npm install to add the package to your project dependencies.

## Red team plugins

Plugins define which adversarial red team attack categories are generated.

## Changelog

Version 1.2 fixed a typo in the readme file.
";

#[test]
fn keeps_blocks_matching_the_query() {
    let out = filter_blocks(DOC, "red teaming", 0.5);
    assert!(out.contains("adversarial prompts"));
    assert!(out.contains("red team attack categories"));
}

#[test]
fn drops_blocks_unrelated_to_the_query() {
    let out = filter_blocks(DOC, "red teaming", 0.5);
    assert!(
        !out.contains("fixed a typo"),
        "irrelevant block kept: {out}"
    );
}

#[test]
fn preserves_document_order_of_surviving_blocks() {
    let out = filter_blocks(DOC, "red teaming", 0.5);
    let a = out.find("adversarial prompts").unwrap();
    let b = out.find("red team attack categories").unwrap();
    assert!(a < b, "block order scrambled");
}

#[test]
fn keep_ratio_of_one_returns_everything() {
    let out = filter_blocks(DOC, "red teaming", 1.0);
    assert!(out.contains("fixed a typo"));
}

#[test]
fn empty_query_returns_input_unchanged() {
    assert_eq!(filter_blocks(DOC, "", 0.5), DOC);
}

#[test]
fn never_returns_empty_output() {
    // Even a query matching nothing keeps the single best block, so the user
    // gets something rather than a silent empty file.
    let out = filter_blocks(DOC, "quantum chromodynamics", 0.01);
    assert!(!out.trim().is_empty());
}
