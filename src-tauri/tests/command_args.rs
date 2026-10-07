//! Guard rail (architecture review, 2026-10-07): `command_registration.rs`
//! checks command *names* and `type_mirror.rs` checks *return* types, but
//! nothing checked the argument keys `src/lib/api.ts` sends against the Rust
//! `#[tauri::command]` parameters. A typo (`minWordcount`), a forgotten key
//! (a new `background: bool`), or a stale one compiles clean on both sides
//! and fails only when the command is invoked, with a Tauri arg error.
//!
//! Scope, deliberately: only the commands `api.ts` calls (the debug page uses
//! raw `invoke()` on purpose — `src/CLAUDE.md` — and four commands have no TS
//! caller). Key *names* only, as exact set equality both ways: an `Option<T>`
//! parameter must still be sent as `null` (`src-tauri/CLAUDE.md`). Parameters
//! Tauri injects (`AppHandle`, `State<…>`, windows) are skipped.
//!
//! Both sides are read from source text at test time. The scan is strict by
//! design: an `invoke(` call it cannot parse panics instead of being skipped,
//! because a guard that quietly checks nothing reads exactly like a pass.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Parameter types Tauri fills in itself; they never appear in an `invoke` payload.
const INJECTED: [&str; 3] = ["AppHandle", "State<", "Window"];

fn camel_case(snake: &str) -> String {
    let mut parts = snake.split('_');
    let mut out = parts.next().unwrap_or("").to_string();
    for part in parts {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

/// Index of the bracket closing the one at `open`, over `( { [` only.
fn matching_close(s: &str, open: usize) -> Option<usize> {
    let mut depth = 0i32;
    for (i, c) in s.char_indices().skip_while(|(i, _)| *i < open) {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

/// Splits on commas that are not nested inside brackets. `angle` also counts
/// `<`/`>` (Rust generics such as `HashMap<A, B>`); TS expressions can contain
/// a bare `<`, so it is off there.
fn split_top_level(s: &str, angle: bool) -> Vec<String> {
    let mut parts = vec![];
    let mut depth = 0i32;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            '<' if angle => depth += 1,
            '>' if angle => depth -= 1,
            ',' if depth == 0 => {
                parts.push(std::mem::take(&mut current));
                continue;
            }
            _ => {}
        }
        current.push(c);
    }
    if !current.trim().is_empty() {
        parts.push(current);
    }
    parts
}

/// command name -> camelCased non-injected parameter names, from every
/// `#[tauri::command]` function in the given Rust source text.
fn rust_commands(source: &str) -> BTreeMap<String, BTreeSet<String>> {
    let mut commands = BTreeMap::new();
    let mut rest = source;
    while let Some(at) = rest.find("#[tauri::command") {
        let after_attr = &rest[at..];
        let attr_end = after_attr.find(']').expect("unterminated #[tauri::command]");
        assert!(
            !after_attr[..attr_end].contains("rename_all"),
            "a #[tauri::command(rename_all = …)] changes how argument keys are spelled — teach this guard before using it"
        );
        let body = &after_attr[attr_end..];
        let fn_at = body.find("fn ").expect("#[tauri::command] with no fn after it");
        let after_fn = &body[fn_at + 3..];
        let paren = after_fn.find('(').expect("command fn has no parameter list");
        let name = after_fn[..paren].trim().to_string();
        let close = matching_close(after_fn, paren).expect("unbalanced parameter list");

        let mut params = BTreeSet::new();
        for param in split_top_level(&after_fn[paren + 1..close], true) {
            let param = param.trim();
            if param.is_empty() {
                continue;
            }
            let (pname, ty) = param.split_once(':').unwrap_or_else(|| panic!("unparseable parameter {param:?} of {name}"));
            if INJECTED.iter().any(|marker| ty.contains(marker)) {
                continue;
            }
            params.insert(camel_case(pname.trim().trim_start_matches("mut ").trim()));
        }
        commands.insert(name, params);
        rest = &after_fn[close..];
    }
    commands
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == '$')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '$')
}

/// (command name, argument keys) for every `invoke("name", { … })` call in
/// `api.ts`-shaped text. Panics on any `invoke` call it cannot read.
fn api_invocations(text: &str) -> Vec<(String, BTreeSet<String>)> {
    // Comments mention `invoke()` in prose; they are not calls.
    let code: String =
        text.lines().filter(|l| !l.trim_start().starts_with("//")).collect::<Vec<_>>().join("\n");

    let mut calls = vec![];
    let mut seen = 0usize;
    let mut from = 0usize;
    while let Some(rel) = code[from..].find("invoke") {
        let at = from + rel;
        from = at + "invoke".len();
        let prev_is_ident = code[..at].chars().next_back().is_some_and(|c| c.is_alphanumeric() || c == '_');
        let tail = &code[from..];
        if prev_is_ident || !(tail.starts_with('(') || tail.starts_with('<')) {
            continue; // the import, a longer identifier, …
        }
        seen += 1;

        // `invoke<T>(`: skip the type argument.
        let mut open = from;
        if tail.starts_with('<') {
            let mut depth = 0i32;
            for (i, c) in tail.char_indices() {
                match c {
                    '<' => depth += 1,
                    '>' => {
                        depth -= 1;
                        if depth == 0 {
                            open = from + i + 1;
                            break;
                        }
                    }
                    _ => {}
                }
            }
        }
        assert!(code[open..].starts_with('('), "invoke call without `(` near {:?}", &code[at..(at + 40).min(code.len())]);
        let args_close = matching_close(&code, open).expect("unbalanced invoke(");
        let args = &code[open + 1..args_close];

        let args = args.trim_start();
        assert!(args.starts_with('"'), "invoke command name is not a string literal: {args:?}");
        let name_end = args[1..].find('"').expect("unterminated command name") + 1;
        let name = args[1..name_end].to_string();
        let after_name = args[name_end + 1..].trim();

        let mut keys = BTreeSet::new();
        if let Some(payload) = after_name.strip_prefix(',') {
            let payload = payload.trim();
            let inner = payload
                .strip_prefix('{')
                .and_then(|p| p.strip_suffix('}'))
                .unwrap_or_else(|| panic!("invoke({name:?}) payload is not an inline {{ … }} object: {payload:?}"));
            for item in split_top_level(inner, false) {
                let item = item.trim();
                if item.is_empty() {
                    continue;
                }
                let key = item.split_once(':').map_or(item, |(k, _)| k.trim());
                assert!(is_identifier(key), "invoke({name:?}) key {key:?} is not a plain identifier (from {item:?})");
                keys.insert(key.to_string());
            }
        } else {
            assert!(after_name.is_empty(), "unexpected text after invoke({name:?} …): {after_name:?}");
        }
        calls.push((name, keys));
    }
    assert_eq!(calls.len(), seen, "parsed fewer invoke calls than the text contains");
    calls
}

/// Every way the TS calls disagree with the Rust commands, as readable lines.
fn mismatches(calls: &[(String, BTreeSet<String>)], commands: &BTreeMap<String, BTreeSet<String>>) -> Vec<String> {
    let mut problems = vec![];
    for (name, keys) in calls {
        let Some(params) = commands.get(name) else {
            problems.push(format!("api.ts invokes {name:?} but no #[tauri::command] fn has that name"));
            continue;
        };
        let unexpected: Vec<_> = keys.difference(params).collect();
        let missing: Vec<_> = params.difference(keys).collect();
        if !unexpected.is_empty() {
            problems.push(format!("{name}: api.ts sends {unexpected:?}, which the Rust command does not take"));
        }
        if !missing.is_empty() {
            problems.push(format!("{name}: the Rust command takes {missing:?}, which api.ts does not send"));
        }
    }
    problems
}

fn read(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn all_rust_commands() -> BTreeMap<String, BTreeSet<String>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut commands = BTreeMap::new();
    for entry in fs::read_dir(&dir).expect("read src/") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) == Some("rs") {
            let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            commands.extend(rust_commands(&text));
        }
    }
    commands
}

#[test]
fn api_ts_argument_keys_match_the_rust_command_parameters() {
    let commands = all_rust_commands();
    let calls = api_invocations(&read("../src/lib/api.ts"));

    // The scan must have found something to check on both sides — an empty
    // result means the scan broke, not that everything matches.
    assert!(calls.len() >= 15, "only {} invoke calls parsed from api.ts — the scan broke", calls.len());
    assert!(commands.len() >= 15, "only {} #[tauri::command] fns found — the scan broke", commands.len());
    assert!(commands.get("fetch_chapter").is_some_and(|p| p.contains("background")), "scan lost fetch_chapter's params");

    let problems = mismatches(&calls, &commands);
    assert!(problems.is_empty(), "api.ts and the Rust commands disagree:\n  {}", problems.join("\n  "));
}

// ---- the scan itself, against inputs that must (not) be flagged ----

const RUST_SAMPLE: &str = r#"
    #[tauri::command]
    pub async fn fetch_chapter(
        app: tauri::AppHandle,
        db: tauri::State<'_, db::Db>,
        story_id: i64,
        chapter_num: i64,
        background: bool,
    ) -> Result<ChapterFetched, String> {
        todo!()
    }

    #[tauri::command]
    fn search_ffn(category: Option<String>, min_word_count: i64, filters: HashMap<String, i64>) -> i64 { 0 }
"#;

#[test]
fn rust_scan_reads_camel_cased_params_and_skips_injected_ones() {
    let commands = rust_commands(RUST_SAMPLE);
    let set = |keys: &[&str]| keys.iter().map(|k| k.to_string()).collect::<BTreeSet<_>>();

    assert_eq!(commands["fetch_chapter"], set(&["storyId", "chapterNum", "background"]));
    // A comma inside `HashMap<String, i64>` must not split the parameter.
    assert_eq!(commands["search_ffn"], set(&["category", "minWordCount", "filters"]));
}

#[test]
fn ts_scan_reads_shorthand_expressions_commas_and_generic_invoke() {
    let ts = r#"
        // Keeps `invoke()` string command names in one place.
        import { invoke } from "@tauri-apps/api/core";
        export const a = () => invoke("list_fandoms");
        export const b = (c: string) => invoke("index_ffn_category", { category: c ?? null });
        export const d = () => invoke<ChapterFetched>("fetch_chapter", { storyId, chapterNum: f(1, 2), background });
    "#;
    let calls = api_invocations(ts);
    let keys = |i: usize| calls[i].1.iter().cloned().collect::<Vec<_>>();

    assert_eq!(calls.iter().map(|c| c.0.as_str()).collect::<Vec<_>>(), ["list_fandoms", "index_ffn_category", "fetch_chapter"]);
    assert!(keys(0).is_empty());
    assert_eq!(keys(1), ["category"]);
    assert_eq!(keys(2), ["background", "chapterNum", "storyId"]);
}

#[test]
fn a_misspelled_forgotten_or_stale_key_and_an_unknown_command_are_all_flagged() {
    let commands = rust_commands(RUST_SAMPLE);
    let flagged = |ts: &str| mismatches(&api_invocations(ts), &commands);

    assert!(flagged(r#"invoke("fetch_chapter", { storyId, chapterNum, background })"#).is_empty());
    // typo
    assert_eq!(flagged(r#"invoke("search_ffn", { category, minWordcount, filters })"#).len(), 2);
    // forgotten `background`
    assert_eq!(flagged(r#"invoke("fetch_chapter", { storyId, chapterNum })"#).len(), 1);
    // key the command no longer takes
    assert_eq!(flagged(r#"invoke("fetch_chapter", { storyId, chapterNum, background, extra })"#).len(), 1);
    // no such command
    assert_eq!(flagged(r#"invoke("fetch_chapters", { storyId })"#).len(), 1);
}

#[test]
#[should_panic(expected = "not an inline")]
fn an_invoke_payload_the_scan_cannot_read_panics_instead_of_passing() {
    api_invocations(r#"invoke("fetch_chapter", args)"#);
}

#[test]
#[should_panic(expected = "rename_all")]
fn rename_all_on_a_command_panics_until_the_guard_learns_it() {
    rust_commands("#[tauri::command(rename_all = \"snake_case\")]\nfn x(a_b: i64) {}");
}
