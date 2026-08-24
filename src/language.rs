//! Heuristic language guesser for untagged fenced code blocks.
//!
//! Returns a short canonical tag suitable for ```` ```lang ```` fences
//! (e.g. `"rust"`, `"python"`, `"bash"`), or `None` when the body is
//! too short or too ambiguous.
//!
//! Cheap, deterministic, single pass. No regex backtracking.
//!
//! Strategy: cheap structural pre-counts → four decisive short-circuits
//! (HTML, JSON, Dockerfile, shell one-liner) → per-line keyword scoring
//! → CSS / YAML / JSON disambiguation → winner selection with a margin.

/// Guess the language of a fenced code block body.
pub fn guess_language(body: &str) -> Option<&'static str> {
    let trimmed = body.trim();
    if trimmed.len() < 4 {
        return None;
    }

    // Cheap structural counts (single pass).
    let mut braces = 0usize;
    let mut semis = 0usize;
    let mut bytes = 0usize;
    let mut quote_pairs = 0usize;
    for ch in trimmed.chars() {
        bytes += ch.len_utf8();
        match ch {
            '{' | '[' => braces += 1,
            '}' | ']' => braces += 1,
            ';' => semis += 1,
            '\n' => {}
            '"' => quote_pairs += 1,
            _ => {}
        }
    }
    let arrows = count_byte_pair(trimmed, b"=>");

    // ---------- Decisive short-circuits ----------

    // HTML / XML prologue.
    let head: String = trimmed
        .chars()
        .take(256)
        .flat_map(|c| c.to_lowercase())
        .collect();
    if head.starts_with("<!doctype") || head.starts_with("<?xml") || head.starts_with("<html") {
        return Some("html");
    }

    // Shebang.
    let first_line = trimmed.lines().next().unwrap_or("");
    if first_line.starts_with("#!") {
        if first_line.contains("python") {
            return Some("python");
        }
        if first_line.contains("ruby") || first_line.contains("rake") {
            return Some("ruby");
        }
        if first_line.contains("perl") {
            return Some("perl");
        }
        if first_line.contains("node") || first_line.contains("deno") {
            return Some("javascript");
        }
        if first_line.contains("bash") || first_line.contains("/sh") || first_line.contains("zsh") {
            return Some("bash");
        }
        return Some("shell");
    }

    // JSON. Decisive when the body is brace-balanced and most or all of
    // its keys are double-quoted strings separated by colons. Comments
    // (//, /*) and unquoted identifiers disqualify it.
    if let Some(lang) = detect_json(trimmed, braces, semis, quote_pairs) {
        return Some(lang);
    }

    // Dockerfile. Distinctive: `FROM <image> [AS <name>]` near the top,
    // optionally preceded by `#####` stage banners, plus RUN/COPY/WORKDIR/
    // CMD directives. Body always starts with `FROM` or a `#` comment.
    if detect_dockerfile(trimmed).is_some() {
        return Some("dockerfile");
    }

    // Shell one-liner. Catch the common `npm i -g vercel` /
    // `vercel --prod` / `docker run ...` / `cd foo && make` pattern:
    // short body, starts with a known command name or `$VAR=`.
    if trimmed.lines().count() <= 2 && bytes < 200 {
        if let Some(lang) = detect_shell_oneliner(trimmed) {
            return Some(lang);
        }
    }

    // ---------- Keyword scoring ----------

    let mut scores: [(&str, i32); 18] = [
        ("rust", 0),
        ("python", 0),
        ("javascript", 0),
        ("typescript", 0),
        ("go", 0),
        ("java", 0),
        ("c", 0),
        ("cpp", 0),
        ("ruby", 0),
        ("php", 0),
        ("html", 0),
        ("css", 0),
        ("json", 0),
        ("yaml", 0),
        ("toml", 0),
        ("sql", 0),
        ("bash", 0),
        ("markdown", 0),
    ];

    let mut rust_attr = 0i32;
    let mut py_def = 0i32;
    let mut py_import = 0i32;
    let mut js_signal = 0i32;
    let mut ts_type = 0i32;
    let mut ts_interface = 0i32;
    let mut go_func = 0i32;
    let mut go_pkg = 0i32;
    let mut java_class = 0i32;
    let mut java_ann = 0i32;
    let mut c_include = 0i32;
    let mut c_typedef = 0i32;
    let mut cpp_using = 0i32;
    let mut cpp_template = 0i32;
    let mut cpp_cout = 0i32;
    let mut rb_def = 0i32;
    let mut rb_do = 0i32;
    let mut rb_end = 0i32;
    let mut php_open = 0i32;
    let mut php_dollar = 0i32;
    let mut html_tag = 0i32;
    let mut css_rule = 0i32;
    let mut css_at = 0i32;
    let mut json_brace = 0i32;
    let mut yaml_kv = 0i32;
    let mut toml_sec = 0i32;
    let mut sql_kw = 0i32;
    let mut sh_if = 0i32;
    let mut sh_fi = 0i32;
    let mut sh_echo = 0i32;
    let mut md_fence = 0i32;

    for line in trimmed.lines() {
        let l = line.trim_start();
        let has_indent = line != l;

        // Rust
        if l.starts_with("fn ") || l.starts_with("fn\t") || l.contains(" fn ") || l.contains(" -> ")
        {
            rust_attr += 3;
        }
        if l.starts_with("use ") && l.ends_with(';') {
            rust_attr += 2;
        }
        if l.starts_with("let ") && (l.contains(" mut ") || l.contains("let mut")) {
            rust_attr += 2;
        }
        if l.starts_with("impl ") || l.starts_with("pub fn") || l.starts_with("pub struct") {
            rust_attr += 3;
        }
        if l.starts_with("//!") || l.starts_with("///") {
            rust_attr += 1;
        }
        if l.starts_with("#derive") || l.starts_with("#allow") {
            rust_attr += 2;
        }

        // Python
        if l.starts_with("def ") && l.ends_with(':') {
            py_def += 4;
        }
        if l.starts_with("class ") && l.ends_with(':') {
            py_def += 3;
        }
        if l.starts_with("from ") && l.contains(" import ") {
            py_import += 3;
        }
        if l.starts_with("import ") && !l.contains(';') {
            py_import += 2;
        }
        if l.starts_with("if __name__") || l.starts_with("def __init__") {
            py_def += 3;
        }
        if l.starts_with("    ") && l.trim_end().ends_with(':') && py_def > 0 {
            py_def += 1;
        }

        // JavaScript. We count BOTH arrow-assignment shape
        // (`const f = () =>`) AND promise/method-chaining shape
        // (`.then(`, `.catch(`) so the Prisma seed example is picked up.
        if l.starts_with("const ") || l.starts_with("let ") {
            js_signal += 1;
        }
        if l.starts_with("function ") || l.contains(" function ") {
            js_signal += 2;
        }
        if l.starts_with("import ") && l.contains(" from ") {
            js_signal += 2;
        }
        if l.starts_with("export ") || l.starts_with("export\t") {
            js_signal += 2;
        }
        if l.starts_with("async ") || l.starts_with("async\t") || l.contains("async (") {
            js_signal += 1;
        }
        if l.starts_with("await ") {
            js_signal += 1;
        }
        if l.contains(".then(") || l.contains(".catch(") {
            js_signal += 2;
        }
        if l.contains("=>") && !l.starts_with("//") {
            js_signal += 1;
        }
        if l.starts_with("console.") || l.starts_with("process.") || l.starts_with("require(") {
            js_signal += 1;
        }

        // TypeScript
        if l.starts_with("interface ") || l.starts_with("type ") {
            ts_type += 3;
        }
        if l.contains(": string")
            || l.contains(": number")
            || l.contains(": boolean")
            || l.contains(": void")
            || l.contains(": null")
        {
            ts_type += 2;
        }
        if l.starts_with("implements ") {
            ts_interface += 2;
        }
        if l.contains(" as ") && (l.contains("string") || l.contains("number")) {
            ts_type += 1;
        }

        // Go
        if l.starts_with("func ") {
            go_func += 4;
        }
        if l.starts_with("package ") {
            go_pkg += 3;
        }
        if l.starts_with("import (") || l.starts_with("import \"") {
            go_func += 2;
        }
        if l.starts_with("go ") && (l.contains("func") || l.contains("chan") || l.contains("<-")) {
            go_func += 1;
        }

        // Java
        if l.starts_with("public class ") || (l.starts_with("class ") && l.ends_with('{')) {
            java_class += 3;
        }
        if l.starts_with("@Override") || l.starts_with("@Test") || l.starts_with("@Autowired") {
            java_ann += 3;
        }
        if l.starts_with("System.out.") {
            java_class += 2;
        }
        if l.contains("public static void main") {
            java_class += 4;
        }

        // C
        if l.starts_with("#include") {
            c_include += 4;
        }
        if l.starts_with("typedef ") {
            c_typedef += 3;
        }
        if l.starts_with("printf(") || l.starts_with("scanf(") {
            c_typedef += 2;
        }
        if l.starts_with("int main") || l.starts_with("void main") {
            c_typedef += 3;
        }

        // C++
        if l.starts_with("#include") && l.contains("<iostream>") {
            cpp_using += 4;
        }
        if l.starts_with("std::") {
            cpp_using += 3;
        }
        if l.starts_with("#include")
            && (l.contains("<vector>")
                || l.contains("<string>")
                || l.contains("<map>")
                || l.contains("<algorithm>"))
        {
            cpp_using += 3;
        }
        if l.starts_with("new ") || l.contains(" delete ") {
            cpp_using += 2;
        }
        if l.starts_with("template<") || l.starts_with("template <") {
            cpp_template += 4;
        }
        if l.starts_with("using namespace ") {
            cpp_using += 2;
        }
        if l.starts_with("cout") || l.starts_with("cin >>") {
            cpp_cout += 2;
        }

        // Ruby
        if l.starts_with("def ") && !l.ends_with(':') {
            rb_def += 3;
        }
        if l.starts_with("require ") || l.starts_with("require_relative ") {
            rb_def += 2;
        }
        if l.starts_with("do |") || l.starts_with("do|") {
            rb_do += 3;
        }
        if l == "end" || l.starts_with("end ") {
            rb_end += 1;
        }
        if l.starts_with("puts ") || l.starts_with("p ") {
            rb_def += 1;
        }

        // PHP
        if l.starts_with("<?php") || l.starts_with("<? ") || trimmed.starts_with("<?php") {
            php_open += 4;
        }
        if l.contains("$this->") || l.contains("$_") {
            php_dollar += 2;
        }
        if l.starts_with("namespace ") || (l.starts_with("use ") && l.contains('\\')) {
            php_dollar += 1;
        }

        // HTML
        if l.starts_with("<!DOCTYPE") || l.starts_with("<!doctype") {
            html_tag += 4;
        }
        if l.starts_with("<html") || l.starts_with("<head") || l.starts_with("<body") {
            html_tag += 3;
        }
        if l.starts_with("<div")
            || l.starts_with("<span")
            || l.starts_with("<p>")
            || l.starts_with("<p ")
        {
            html_tag += 2;
        }
        if l.starts_with("<a ") || l.starts_with("<img ") || l.starts_with("<script") {
            html_tag += 1;
        }

        // CSS
        if l.ends_with('{') && !l.starts_with('@') && (l.contains('{') || l.contains(' ')) {
            let before_brace = &l[..l.len() - 1];
            if !before_brace.contains(':') {
                css_rule += 3;
            }
        }
        if l.starts_with("@media") || l.starts_with("@import") || l.starts_with("@keyframes") {
            css_at += 4;
        }
        if l.contains(": ")
            && l.ends_with(';')
            && (l.contains("px") || l.contains("em") || l.contains("rem") || l.contains("rgb"))
        {
            css_rule += 2;
        }

        // JSON
        if l == "{" || l == "}" || l == "[" || l == "]" || l == "}," || l == "}," {
            json_brace += 1;
        }
        if l.starts_with('"') && l.contains(':') && l.contains(',') {
            json_brace += 2;
        }

        // YAML
        if has_indent && l.contains(": ") && !l.trim_start().starts_with('#') {
            yaml_kv += 2;
        }
        if l.starts_with("- ") && l.contains(": ") {
            yaml_kv += 2;
        }
        if l == "---" || l == "..." {
            yaml_kv += 3;
        }

        // TOML
        if l.starts_with('[') && l.ends_with(']') && !l.starts_with("[]") {
            toml_sec += 3;
        }

        // SQL. The leading keywords are the only reliable signal here —
        // mid-line `FROM` / `WHERE` / `JOIN` also fire in JS/TS ES-module
        // imports (`import x from "..."`) and in many prose lines, so we
        // require the line to begin with a SQL keyword (after optional
        // whitespace). That keeps the cheap branch cheap.
        let upper = l.to_uppercase();
        if upper.starts_with("SELECT ")
            || upper.starts_with("INSERT ")
            || upper.starts_with("UPDATE ")
            || upper.starts_with("DELETE ")
            || upper.starts_with("CREATE ")
            || upper.starts_with("ALTER ")
            || upper.starts_with("DROP ")
            || upper.starts_with("WITH ")
        {
            sql_kw += 4;
        }
        // Mid-line `FROM`/`WHERE`/`JOIN` only score when the body
        // already shows a leading SQL keyword somewhere — without that,
        // every JS/TS import would qualify.
        if sql_kw > 0
            && (upper.contains(" FROM ") || upper.contains(" WHERE ") || upper.contains(" JOIN "))
        {
            sql_kw += 2;
        }
        if upper.ends_with(';') && sql_kw > 0 {
            sql_kw += 1;
        }

        // Shell
        if l.starts_with("#!/") || l.starts_with("if [") || l.starts_with("if [[") {
            sh_if += 3;
        }
        if l == "fi" || l == "fi;" {
            sh_fi += 2;
        }
        if l == "done" || l == "esac" {
            sh_fi += 2;
        }
        if l.starts_with("echo ") || l.starts_with("echo\t") {
            sh_echo += 1;
        }
        if l.starts_with("export ") && !l.contains(" from ") {
            sh_echo += 1;
        }

        // Markdown
        if l.starts_with("# ") || l.starts_with("## ") || l.starts_with("### ") {
            md_fence += 3;
        }
        if l.starts_with("```") {
            md_fence += 4;
        }
        if l.starts_with("> ") || (l.starts_with("- ") && !l.contains(':')) {
            md_fence += 1;
        }
    }

    // Aggregate scores. CPP-specific signals zero out the generic C score.
    let c_total = if cpp_using + cpp_template + cpp_cout > 0 {
        0
    } else {
        c_include + c_typedef
    };
    scores[0].1 = rust_attr;
    scores[1].1 = py_def + py_import;
    scores[2].1 = js_signal;
    scores[3].1 = ts_type + ts_interface;
    scores[4].1 = go_func + go_pkg;
    scores[5].1 = java_class + java_ann;
    scores[6].1 = c_total;
    scores[7].1 = cpp_using + cpp_template + cpp_cout;
    scores[8].1 = rb_def + rb_do + rb_end;
    scores[9].1 = php_open + php_dollar;
    scores[10].1 = html_tag;
    scores[11].1 = css_rule + css_at;
    scores[12].1 = json_brace;
    scores[13].1 = yaml_kv;
    scores[14].1 = toml_sec;
    scores[15].1 = sql_kw;
    scores[16].1 = sh_if + sh_fi + sh_echo;
    scores[17].1 = md_fence;

    // Punctuation reweighting. JSON almost always has balanced braces and
    // colons but few keywords; if braces/colons dominate and `json_brace`
    // is the only meaningful keyword signal, bias to JSON.
    if json_brace >= 4 && braces >= 4 && semis == 0 {
        scores[12].1 += 5;
    }

    // CSS rule-shape detection — must NOT fire when the body also shows
    // any JS/TS/Python/etc. signal, even if those signals are weak.
    // (Original bug: prisma seed `import { ... } ... await db.example.upsert({
    //   where: { id }, ... })` was tagged as CSS.)
    let css_has_rule_shape = {
        let mut depth = 0i32;
        let mut opened_on_selector = false;
        for line in trimmed.lines() {
            let l = line.trim_start();
            let has_open = l.ends_with('{');
            if has_open && !l.starts_with('@') && !l.starts_with('{') && depth == 0 {
                let before_brace = &l[..l.len() - 1];
                if !before_brace.contains(':') {
                    opened_on_selector = true;
                }
            }
            for ch in l.chars() {
                match ch {
                    '{' => depth += 1,
                    '}' => depth -= 1,
                    _ => {}
                }
            }
        }
        opened_on_selector && depth == 0
    };
    let code_lang_signal = rust_attr
        + py_def
        + py_import
        + go_func
        + go_pkg
        + java_class
        + java_ann
        + c_include
        + c_typedef
        + cpp_using
        + cpp_template
        + cpp_cout
        + rb_def
        + rb_do
        + rb_end
        + php_open
        + php_dollar
        + js_signal
        + ts_type
        + ts_interface;
    if css_has_rule_shape && code_lang_signal == 0 && semis >= 1 {
        scores[11].1 += semis as i32 * 3 + 4;
    }

    // Strong structural cues override weak keyword ties.
    if arrows >= 2 && js_signal > 0 {
        scores[2].1 += 2;
    }

    // Markdown-like fenced block inside the body? If it has many `##`
    // headings and very little code punctuation, it's md.
    if (trimmed.contains("\n## ") || trimmed.starts_with("# "))
        && trimmed.matches('#').count() >= 2
        && braces == 0
        && semis == 0
    {
        return Some("markdown");
    }

    // Pick the winner. Require a margin over the runner-up to avoid
    // false positives on short snippets.
    let mut best_idx = 0usize;
    let mut best_score = i32::MIN;
    for (i, (_, s)) in scores.iter().enumerate() {
        if *s > best_score {
            best_score = *s;
            best_idx = i;
        }
    }

    if best_score < 3 {
        return None;
    }

    let mut second = i32::MIN;
    for (i, (_, s)) in scores.iter().enumerate() {
        if i != best_idx && *s > second {
            second = *s;
        }
    }
    if best_score - second < 2 && second >= 3 {
        return None;
    }

    Some(scores[best_idx].0)
}

// ---------- Helpers ----------

fn count_byte_pair(s: &str, pair: &[u8; 2]) -> usize {
    s.as_bytes().windows(2).filter(|w| *w == pair).count()
}

/// Detect JSON: braces must balance, keys must be quoted, no `//` or
/// `/*` comments. We allow YAML-style leading `---` only if the body
/// otherwise looks strongly JSON-shaped (rare; excluded).
fn detect_json(
    trimmed: &str,
    braces: usize,
    semis: usize,
    quote_pairs: usize,
) -> Option<&'static str> {
    if braces < 2 || braces % 2 != 0 {
        return None;
    }
    if trimmed.contains("//") || trimmed.contains("/*") {
        return None;
    }
    // Must start with `{` or `[`.
    let first_non_ws = trimmed.chars().find(|c| !c.is_whitespace())?;
    if first_non_ws != '{' && first_non_ws != '[' {
        return None;
    }
    // Count lines that begin with a double-quoted key. ≥2 such lines and
    // zero unquoted-identifier keys ⇒ JSON.
    let mut quoted_key_lines = 0usize;
    let mut unquoted_key_lines = 0usize;
    for line in trimmed.lines() {
        let l = line.trim_start();
        if l.is_empty() || l.starts_with("//") {
            continue;
        }
        if let Some(rest) = l.strip_prefix('"') {
            if rest.find('"').is_some() && rest.find(':').is_some() {
                quoted_key_lines += 1;
            }
        } else if l.contains(':') && !l.starts_with('#') && !l.starts_with("//") {
            // An unquoted key like `name: wcl` → not JSON, that's YAML.
            // Allow JSON-style numeric/array lines.
            let has_brace_or_bracket =
                l.contains('{') || l.contains('}') || l.contains('[') || l.contains(']');
            let starts_with_digit = l
                .chars()
                .next()
                .map(|c| c.is_ascii_digit() || c == '-' || c == 't' || c == 'f' || c == 'n')
                .unwrap_or(false);
            if !has_brace_or_bracket && !starts_with_digit && !l.ends_with(',') {
                unquoted_key_lines += 1;
            }
        }
    }
    if quoted_key_lines >= 2 && unquoted_key_lines == 0 && semis == 0 {
        return Some("json");
    }
    // Small arrays of primitives, e.g. `[1, 2, 3]` or `["a", "b"]` —
    // declared by quote_pairs alone. We require brace parity plus
    // ≥4 quoted-token-ish commas inside square brackets.
    if first_non_ws == '[' && braces == 2 && semis == 0 && quote_pairs >= 2 && trimmed.contains(',')
    {
        return Some("json");
    }
    let _ = braces;
    None
}

/// Detect Dockerfile: `FROM ...` (optionally preceded by `#`/`#####`
/// stage banners) plus at least one other Dockerfile directive.
fn detect_dockerfile(trimmed: &str) -> Option<()> {
    let mut saw_from = false;
    let mut other_directive = false;
    for line in trimmed.lines() {
        let l = line.trim_start();
        if l.is_empty() {
            continue;
        }
        if !saw_from
            && (l.starts_with("FROM ")
                || l.starts_with("FROM\t")
                || l.starts_with("FROM$")
                || l == "FROM")
        {
            saw_from = true;
            continue;
        }
        // Stage banner comments above a FROM are common: `##### BUILDER`.
        if l.starts_with('#') {
            continue;
        }
        if l.starts_with("RUN ")
            || l.starts_with("RUN\t")
            || l.starts_with("WORKDIR ")
            || l.starts_with("COPY ")
            || l.starts_with("CMD ")
            || l.starts_with("ENTRYPOINT ")
            || l.starts_with("ENV ")
            || l.starts_with("ARG ")
            || l.starts_with("EXPOSE ")
            || l.starts_with("LABEL ")
            || l.starts_with("USER ")
            || l.starts_with("VOLUME ")
            || l.starts_with("ADD ")
            || l.starts_with("ONBUILD ")
        {
            other_directive = true;
        }
    }
    if saw_from && other_directive {
        Some(())
    } else {
        None
    }
}

/// Detect a short shell command. Returns `Some("bash")` when the first
/// token is a known command name, or when the line is `export KEY=val`
/// or `KEY=val cmd…` (an assignment chained to a command). A bare
/// `KEY=val` line is intentionally NOT flagged — that's `.env`-file
/// content, not a shell command.
fn detect_shell_oneliner(trimmed: &str) -> Option<&'static str> {
    let line = trimmed.lines().next()?.trim();
    if line.is_empty() {
        return None;
    }
    // Skip if it looks like code (braces / semicolons / arrow functions
    // are very unusual in a one-line shell command).
    if line.contains('{') || line.contains('}') || line.contains(';') || line.contains("=>") {
        return None;
    }
    let first = line.split_whitespace().next()?;
    // Explicit `export FOO=bar`.
    if first == "export" {
        return Some("bash");
    }
    // Bare assignment `FOO=bar` alone → ambiguous (could be .env). Only
    // treat as bash when it's chained to a command: `FOO=bar cmd …` or
    // joined with `&&` / `||` / `;` / `|` (the latter two already
    // excluded above, so just check for `&&` / `||` / whitespace after
    // the value).
    if first.contains('=')
        && first
            .chars()
            .next()
            .map(|c| c.is_ascii_uppercase() || c == '_' || c == '$')
            .unwrap_or(false)
        && !first.starts_with("==")
    {
        // Bare `FOO=bar` alone → ambiguous (.env). Only treat as bash
        // when there's a second token (the command being prefixed) or a
        // chained operator after the value.
        if line.split_whitespace().count() >= 2 {
            return Some("bash");
        }
        return None;
    }
    // First token must be alphanumeric (no leading quote, no `<` HTML).
    if !first
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.' || c == '/')
    {
        return None;
    }
    // Strip path prefix: `/usr/bin/curl` → `curl`.
    let cmd = first.rsplit('/').next().unwrap_or(first);
    const SHELL_CMDS: &[&str] = &[
        "npm",
        "npx",
        "pnpm",
        "yarn",
        "bun",
        "node",
        "deno",
        "tsc",
        "tsx",
        "ts-node",
        "docker",
        "docker-compose",
        "podman",
        "kubectl",
        "helm",
        "git",
        "gh",
        "curl",
        "wget",
        "cd",
        "ls",
        "pwd",
        "mkdir",
        "rm",
        "rmdir",
        "mv",
        "cp",
        "cat",
        "less",
        "more",
        "echo",
        "printf",
        "touch",
        "chmod",
        "chown",
        "ln",
        "tar",
        "gzip",
        "gunzip",
        "zip",
        "unzip",
        "grep",
        "rg",
        "ag",
        "sed",
        "awk",
        "cut",
        "sort",
        "uniq",
        "wc",
        "head",
        "tail",
        "make",
        "cmake",
        "cargo",
        "rustc",
        "go",
        "python",
        "python3",
        "pip",
        "pip3",
        "pipx",
        "brew",
        "apt",
        "apt-get",
        "yum",
        "dnf",
        "pacman",
        "snap",
        "vercel",
        "netlify",
        "ntl",
        "fly",
        "railway",
        "heroku",
        "wrangler",
        "ssh",
        "scp",
        "rsync",
        "ping",
        "traceroute",
        "sudo",
        "su",
        "alias",
        "source",
        "kill",
        "ps",
        "top",
        "htop",
        "df",
        "du",
        "open",
        "xdg-open",
    ];
    if SHELL_CMDS.contains(&cmd) {
        return Some("bash");
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shebang_python() {
        assert_eq!(
            guess_language("#!/usr/bin/env python3\nprint(1)"),
            Some("python")
        );
    }

    #[test]
    fn shebang_bash() {
        assert_eq!(guess_language("#!/bin/bash\necho hi"), Some("bash"));
    }

    #[test]
    fn rust_fn() {
        let s = "fn main() {\n    let mut x = 1;\n    println!(\"{}\", x);\n}\n";
        assert_eq!(guess_language(s), Some("rust"));
    }

    #[test]
    fn python_def() {
        let s = "def hello(name):\n    print(f\"hi {name}\")\n    return name\n";
        assert_eq!(guess_language(s), Some("python"));
    }

    #[test]
    fn js_arrow() {
        let s = "const add = (a, b) => a + b;\nconsole.log(add(1, 2));\n";
        assert_eq!(guess_language(s), Some("javascript"));
    }

    #[test]
    fn go_pkg() {
        let s = "package main\n\nimport \"fmt\"\n\nfunc main() {\n    fmt.Println(\"hi\")\n}\n";
        assert_eq!(guess_language(s), Some("go"));
    }

    #[test]
    fn sql_select() {
        let s = "SELECT id, name FROM users WHERE active = 1;\n";
        assert_eq!(guess_language(s), Some("sql"));
    }

    #[test]
    fn json_object() {
        let s =
            "{\n  \"name\": \"wcl\",\n  \"version\": \"0.1.0\",\n  \"deps\": [\"a\", \"b\"]\n}\n";
        assert_eq!(guess_language(s), Some("json"));
    }

    #[test]
    fn json_vercel_config() {
        // User-reported: this used to return None.
        let s = "{\n  \"buildCommand\": \"npm run build\",\n  \"devCommand\": \"npm run dev\",\n  \"installCommand\": \"npm install\"\n}\n";
        assert_eq!(guess_language(s), Some("json"));
    }

    #[test]
    fn json_package_json_shape() {
        let s = "{\n  \"name\": \"foo\",\n  \"version\": \"1.0.0\",\n  \"scripts\": { \"dev\": \"vite\" }\n}\n";
        assert_eq!(guess_language(s), Some("json"));
    }

    #[test]
    fn json_array() {
        let s = "[\"a\", \"b\", \"c\"]";
        assert_eq!(guess_language(s), Some("json"));
    }

    #[test]
    fn html_doctype() {
        let s = "<!DOCTYPE html>\n<html><body><p>hi</p></body></html>\n";
        assert_eq!(guess_language(s), Some("html"));
    }

    #[test]
    fn yaml_kv() {
        let s = "---\nname: wcl\nversion: 0.1.0\ndeps:\n  - a\n  - b\n";
        assert_eq!(guess_language(s), Some("yaml"));
    }

    #[test]
    fn css_rule() {
        let s = "body {\n  margin: 0;\n  padding: 0;\n  font-size: 16px;\n}\n";
        assert_eq!(guess_language(s), Some("css"));
    }

    #[test]
    fn css_many_properties_not_yaml() {
        // Regression: many indented `key: value;` lines look like YAML.
        let s = "\
.my-class {\n\
  display: flex;\n\
  flex-direction: column;\n\
  justify-content: center;\n\
  align-items: center;\n\
  background-color: #fff;\n\
  border: 1px solid #e2e8f0;\n\
  border-radius: 0.25rem;\n\
  padding: 1rem;\n\
}\n";
        assert_eq!(guess_language(s), Some("css"));
    }

    #[test]
    fn shell_if_fi() {
        let s = "#!/bin/sh\nif [ \"$x\" = \"1\" ]; then\n  echo yes\nfi\n";
        assert_eq!(guess_language(s), Some("bash"));
    }

    #[test]
    fn shell_npm_global_install() {
        // User-reported: this used to return None.
        assert_eq!(guess_language("npm i -g vercel"), Some("bash"));
    }

    #[test]
    fn shell_vercel_command() {
        assert_eq!(guess_language("vercel --prod"), Some("bash"));
        assert_eq!(
            guess_language("vercel --env DATABASE_URL=YOUR_DATABASE_URL_HERE --yes"),
            Some("bash")
        );
    }

    #[test]
    fn shell_docker_run() {
        assert_eq!(
            guess_language("docker run -p 3000:3000 my-image"),
            Some("bash")
        );
    }

    #[test]
    fn shell_cd_then_make() {
        assert_eq!(guess_language("cd my-project && make"), Some("bash"));
    }

    #[test]
    fn shell_git_one_liner() {
        assert_eq!(
            guess_language("git commit -m \"fix: off-by-one\""),
            Some("bash")
        );
    }

    #[test]
    fn shell_export_assignment() {
        assert_eq!(
            guess_language("export DATABASE_URL=postgres://x"),
            Some("bash")
        );
    }

    #[test]
    fn shell_assignment_not_bash_when_long() {
        // Long pseudo-configs should NOT be tagged bash.
        let s = "DATABASE_URL=postgres://user:pass@host:5432/db\nNODE_ENV=production\nPORT=3000\n";
        // This is an .env file shape — currently untagged. Verify it stays untagged.
        assert_ne!(guess_language(s), Some("bash"));
    }

    #[test]
    fn js_prisma_seed_not_css() {
        // User-reported: this used to be tagged "css".
        let s = "import { db } from \"../src/server/db\";\n\nasync function main() {\n  const id = \"cl9ebqhxk00003b600tymydho\";\n  await db.example.upsert({\n    where: {\n      id,\n    },\n    create: {\n      id,\n    },\n    update: {},\n  });\n}\n\nmain()\n  .then(async () => {\n    await db.$disconnect();\n  })\n  .catch(async (e) => {\n    console.error(e);\n    await db.$disconnect();\n    process.exit(1);\n  });";
        let g = guess_language(s);
        assert_ne!(g, Some("css"), "prisma seed should not be CSS: {:?}", g);
        assert!(
            g == Some("javascript") || g == Some("typescript"),
            "expected js/ts, got {:?}",
            g
        );
    }

    #[test]
    fn js_object_literal_not_css() {
        let s = "\
import { createEnv } from \"@t3-oss/env-nextjs\";\n\
import { z } from \"zod\";\n\
\n\
export const env = createEnv({\n\
  server: {\n\
    TWITTER_API_TOKEN: z.string(),\n\
  },\n\
  runtimeEnv: {\n\
    TWITTER_API_TOKEN: process.env.TWITTER_API_TOKEN,\n\
  },\n\
});\n";
        let g = guess_language(s);
        assert_ne!(g, Some("css"), "env config should not be CSS: {:?}", g);
    }

    #[test]
    fn js_function_not_css() {
        let s = "function add(a, b) {\n  return a + b;\n}\n\nconsole.log(add(1, 2));";
        let g = guess_language(s);
        assert_ne!(g, Some("css"), "plain JS should not be CSS: {:?}", g);
    }

    #[test]
    fn cpp_iostream() {
        let s = "#include <iostream>\nint main() {\n    std::cout << \"hi\" << std::endl;\n    return 0;\n}\n";
        assert_eq!(guess_language(s), Some("cpp"));
    }

    #[test]
    fn dockerfile_multistage() {
        // User-reported: this used to return None.
        let s = "##### DEPENDENCIES\n\nFROM --platform=linux/amd64 node:20-alpine AS deps\nRUN apk add --no-cache libc6-compat openssl\nWORKDIR /app\n\nCOPY package.json yarn.lock* package-lock.json* pnpm-lock.yaml* ./\n\nRUN \\\n    if [ -f yarn.lock ]; then yarn --frozen-lockfile; \\\n    elif [ -f package-lock.json ]; then npm ci; \\\n    elif [ -f pnpm-lock.yaml ]; then npm install -g pnpm && pnpm i; \\\n    else echo \"Lockfile not found.\" && exit 1; \\\n    fi\n\n##### BUILDER\n\nFROM --platform=linux/amd64 node:20-alpine AS builder\nARG DATABASE_URL\nWORKDIR /app\nCOPY --from=deps /app/node_modules ./node_modules\nCOPY . .\n\nENV NODE_ENV production\nCMD [\"server.js\"]\n";
        assert_eq!(guess_language(s), Some("dockerfile"));
    }

    #[test]
    fn dockerfile_simple() {
        let s = "FROM node:20-alpine\nWORKDIR /app\nCOPY package.json .\nRUN npm install\nCMD [\"node\", \"server.js\"]\n";
        assert_eq!(guess_language(s), Some("dockerfile"));
    }

    #[test]
    fn dockerignore_not_dockerfile() {
        // Plain dotfile lists should not be flagged as dockerfile.
        let s = ".env\nDockerfile\n.dockerignore\nnode_modules\nnpm-debug.log\n";
        assert_ne!(guess_language(s), Some("dockerfile"));
    }

    #[test]
    fn too_short_returns_none() {
        assert_eq!(guess_language("hi"), None);
        assert_eq!(guess_language(""), None);
    }

    #[test]
    fn ambiguous_returns_none() {
        let s = "foo\n";
        assert_eq!(guess_language(s), None);
    }

    #[test]
    fn dotenv_line_not_bash() {
        // Multi-line env files: not a shell command, not a CSS file.
        let s = "TWITTER_API_TOKEN=1234567890\n";
        assert_ne!(guess_language(s), Some("bash"));
        assert_ne!(guess_language(s), Some("css"));
    }

    #[test]
    fn ts_trpc_handler_not_sql() {
        // Regression: `import { ... } from "..."` was matching SQL
        // because `from` (lowercased) appeared mid-line.
        let s = "import { type NextApiRequest, type NextApiResponse } from \"next\";\nimport { appRouter, createCaller } from \"../../../server/api/root\";\n\nconst userByIdHandler = async (req: NextApiRequest, res: NextApiResponse) => {\n  const ctx = await createTRPCContext({ req, res });\n  try {\n    const { id } = req.query;\n    const user = await caller.user.getById(id);\n    res.status(200).json(user);\n  } catch (cause) {\n    if (cause instanceof TRPCError) {\n      const httpCode = getHTTPStatusCodeFromError(cause);\n      return res.status(httpCode).json(cause);\n    }\n    console.error(cause);\n    res.status(500).json({ message: \"Internal server error\" });\n  }\n};\n\nexport default userByIdHandler;\n";
        let g = guess_language(s);
        assert_ne!(g, Some("sql"), "trpc handler should not be SQL: {:?}", g);
        assert!(
            g == Some("javascript") || g == Some("typescript"),
            "expected js/ts, got {:?}",
            g
        );
    }
}
