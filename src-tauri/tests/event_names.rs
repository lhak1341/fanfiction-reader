//! Guard rail for architecture review candidate 4 (2026-10-07): the fetch
//! events Rust emits (`app.emit("chapter-fetched", ..)`, `fetch.rs`) and the
//! ones `src/lib/api.ts` listens for (`defineFetchEvent<..>("chapter-fetched",
//! "chapter-fetch-error")`) are matched only by string. `type_mirror.rs`
//! covers the payload fields and TS's own types cover the correlation keys,
//! but a typo in either side's event *name* compiles clean and just leaves the
//! awaited promise unsettled — which looks like a hang, not an error. Same
//! shape as `command_registration.rs`/`type_mirror.rs`: both sides are read
//! from the real source at test time, no hand-kept parallel list.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Every string literal passed as the first argument to a `.emit(` call in
/// `src/*.rs`.
fn rust_emitted_events() -> BTreeSet<String> {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(&src_dir).expect("read src/ dir") {
        let path = entry.expect("read dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let mut cursor = source.as_str();
        while let Some(i) = cursor.find(".emit(\"") {
            cursor = &cursor[i + ".emit(\"".len()..];
            let end = cursor.find('"').expect("unterminated emit string");
            found.insert(cursor[..end].to_string());
        }
    }
    found
}

/// Both event-name string literals of every `defineFetchEvent<..>(ok, err)`
/// call in non-test `src/lib/*.ts`. The call can wrap across lines, so this
/// scans from the `>(` that closes the generics to the next `)`.
fn ts_listened_events() -> BTreeSet<String> {
    let lib_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../src/lib");
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(&lib_dir).expect("read src/lib dir") {
        let path = entry.expect("read dir entry").path();
        let name = path.file_name().and_then(|f| f.to_str()).unwrap_or_default();
        if !name.ends_with(".ts") || name.ends_with(".test.ts") || name == "tauriEvents.ts" {
            continue;
        }
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let mut cursor = source.as_str();
        while let Some(i) = cursor.find("defineFetchEvent<") {
            cursor = &cursor[i..];
            let open = cursor.find(">(").unwrap_or_else(|| panic!("{name}: `defineFetchEvent<` without `>(`")) + 2;
            let close = cursor[open..].find(')').unwrap_or_else(|| panic!("{name}: unterminated defineFetchEvent call"));
            let args = &cursor[open..open + close];
            let literals: Vec<&str> = args.split('"').skip(1).step_by(2).collect();
            assert_eq!(literals.len(), 2, "{name}: expected exactly two event-name literals in `{args}`");
            found.extend(literals.into_iter().map(str::to_string));
            cursor = &cursor[open + close..];
        }
    }
    found
}

#[test]
fn every_emitted_fetch_event_is_listened_for_and_vice_versa() {
    let emitted = rust_emitted_events();
    let listened = ts_listened_events();

    assert!(!emitted.is_empty(), "scan found no `.emit(\"..\")` calls — the scan itself is broken");
    assert!(!listened.is_empty(), "scan found no `defineFetchEvent` calls — the scan itself is broken");

    let only_rust: Vec<_> = emitted.difference(&listened).collect();
    let only_ts: Vec<_> = listened.difference(&emitted).collect();
    assert!(
        only_rust.is_empty() && only_ts.is_empty(),
        "fetch event names drifted — emitted by Rust but never listened for in TS: {only_rust:?}; listened for in TS but never emitted by Rust: {only_ts:?}"
    );
}

#[test]
fn every_fetched_event_has_its_fetch_error_twin() {
    let emitted = rust_emitted_events();
    for name in &emitted {
        let twin = if let Some(stem) = name.strip_suffix("-fetched") {
            format!("{stem}-fetch-error")
        } else if let Some(stem) = name.strip_suffix("-fetch-error") {
            format!("{stem}-fetched")
        } else {
            panic!("event `{name}` is neither `<kind>-fetched` nor `<kind>-fetch-error`");
        };
        assert!(emitted.contains(&twin), "event `{name}` has no `{twin}` twin");
    }
}
