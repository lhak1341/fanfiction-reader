//! Guard rail for architecture review candidate (2026-07-29): `src/lib/types.ts`
//! hand-mirrors the Rust structs Tauri serializes back to the frontend (see
//! its own top-of-file comment) — nothing previously checked the two sides
//! actually agreed, so a renamed/removed/added Rust field would compile
//! clean and only break at runtime with a silently-`undefined` TS field, or
//! a cryptic serde error. This converts that failure mode into a test-time
//! one, same shape as `command_registration.rs` (settled architecture
//! review candidate 4): both sides are read from the actual source at test
//! time, via small text scans, not a hand-maintained parallel field list.
//!
//! Scoped to the `Serialize` (Rust→TS) structs only — every field of
//! consequence there is one the frontend must be able to read. The
//! `Deserialize` (TS→Rust) side (`StoryFilters`) is deliberately excluded:
//! its fields are `#[serde(default)]`-tolerant of TS omitting them, so the
//! risk shape is different (the one real bug this distinction caught was
//! `bookmarked_only`, a dead TS-settable field that has since moved out of
//! `StoryFilters` into `queries::Scope` — a dead field, not a name mismatch,
//! which no field-name-diffing test could have caught).
//!
//! TS interface names are required to match their Rust struct name exactly
//! (`ChapterFetched` (Rust) <-> `ChapterFetched` (TS), not `ChapterFetchedEvent`)
//! so pairing them up here needs no separate Rust-name-to-TS-name mapping —
//! one more hand-maintained list that could itself drift.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

/// Struct names whose shape the frontend reads, each paired 1:1 with a
/// same-named `interface` in `types.ts`. The file a struct lives in is
/// discovered by scanning `src/*.rs` (see `serialize_types`) rather than
/// recorded here — moving one between files is not a reason for this test
/// to fail.
const MIRRORED_STRUCTS: &[&str] = &[
    "StoryRow",
    "ChapterListItem",
    "ChapterContent",
    "FandomIndexEntry",
    "ChapterFetched",
    "EnsuredChapter",
    "BrowseFetched",
    "FandomIndexFetched",
];

/// `Serialize`-deriving types deliberately outside the mirror, each with the
/// reason it never crosses to TS. Every `Serialize` type in `src/` has to
/// appear in exactly one of these two tables — `every_serialize_type_is_...`
/// below fails on one that appears in neither, so a type that starts
/// crossing the wire can't reach the frontend unmirrored.
const UNMIRRORED_TYPES: &[(&str, &str)] = &[
    (
        "FetchOutcome",
        "dispatch.rs: only the raw-URL `fetch_page`/`fetch_ao3_page` (debug page) return it, and nothing there reads the payload; every typed fetch command returns one of the mirrored `*Fetched` structs instead",
    ),
    (
        "StoryMetadata",
        "ffn.rs: parser output, persisted into SQLite by `dispatch::persist_page`; no command hands it to the frontend",
    ),
    ("ChapterRef", "ffn.rs: parser output, same as `StoryMetadata`"),
    ("Ao3StoryMetadata", "ao3.rs: parser output, same as `StoryMetadata`"),
    ("Ao3ChapterRef", "ao3.rs: parser output, same as `StoryMetadata`"),
];

/// Every `#[derive(.., Serialize, ..)]` type in `src/*.rs`, mapped to the
/// file it lives in. Attribute and doc-comment lines between the derive and
/// the item are skipped, so an intervening `#[serde(rename_all = ..)]` is
/// fine. Enums are collected too: one deriving `Serialize` crosses the wire
/// the same way a struct does, and has to be classified rather than
/// silently escaping both tables above.
fn serialize_types() -> BTreeMap<String, String> {
    let src_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut found = BTreeMap::new();

    for entry in fs::read_dir(&src_dir).expect("read src/ dir") {
        let path = entry.expect("read dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let file = path.file_name().and_then(|f| f.to_str()).expect("file name").to_string();
        let source = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        let lines: Vec<&str> = source.lines().map(str::trim).collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.starts_with("#[derive") || !line.contains("Serialize") {
                continue;
            }
            let item = lines[i + 1..]
                .iter()
                .find(|l| !l.is_empty() && !l.starts_with('#') && !l.starts_with("//"))
                .unwrap_or_else(|| panic!("{file}: `{line}` with no item after it"));
            let Some(rest) = item.split_once("struct ").or_else(|| item.split_once("enum ")).map(|(_, rest)| rest) else {
                continue;
            };
            let name = rest.trim().split(['{', '<', '(', ';', ' ']).next().unwrap_or_default();
            found.insert(name.to_string(), file.clone());
        }
    }
    found
}
/// Extracts `struct {name} { ... }`'s field names from Rust source text.
/// None of these structs nest braces inside a field's type (no inline
/// structs/closures), so naive first-`{`-to-matching-`}` extraction is safe.
/// Skips attribute lines (`#[...]`) and doc-comment lines (`///`/`//`); a
/// field line is everything up to its first `:`, with a leading `pub `
/// trimmed off.
fn rust_struct_fields(source: &str, struct_name: &str) -> BTreeSet<String> {
    let needle = format!("struct {struct_name} {{");
    let start = source.find(&needle).unwrap_or_else(|| panic!("no `{needle}` found"));
    let body_start = start + needle.len();

    let mut depth = 1;
    let mut body_end = body_start;
    for (i, c) in source[body_start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    body_end = body_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    source[body_start..body_end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with("//"))
        .filter_map(|line| line.split_once(':'))
        .map(|(name, _)| name.trim().trim_start_matches("pub ").trim().to_string())
        .collect()
}

/// snake_case -> camelCase, matching serde's `rename_all = "camelCase"`.
fn to_camel_case(snake: &str) -> String {
    let mut parts = snake.split('_');
    let first = parts.next().unwrap_or_default().to_string();
    parts.fold(first, |mut acc, part| {
        let mut chars = part.chars();
        if let Some(c) = chars.next() {
            acc.push(c.to_ascii_uppercase());
            acc.push_str(chars.as_str());
        }
        acc
    })
}

/// Extracts `interface {name} { ... }`'s field names from `types.ts`. A
/// field line is everything up to its first `:` or `?`, comments (`//`) and
/// blank lines skipped. Brace-depth tracked (not just first-`}`-wins) since
/// a doc comment can itself contain literal `{`/`}` (e.g. `ChapterListItem`'s
/// `/works/{id}/chapters/{id}` example).
fn ts_interface_fields(source: &str, interface_name: &str) -> BTreeSet<String> {
    let needle = format!("interface {interface_name} {{");
    let start = source.find(&needle).unwrap_or_else(|| panic!("no `{needle}` found in types.ts"));
    let body_start = start + needle.len();

    let mut depth = 1;
    let mut body_end = body_start;
    for (i, c) in source[body_start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    body_end = body_start + i;
                    break;
                }
            }
            _ => {}
        }
    }

    source[body_start..body_end]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with("//") && !line.starts_with("/*") && !line.starts_with('*'))
        .filter_map(|line| line.find([':', '?']).map(|i| line[..i].trim().to_string()))
        .collect()
}

#[test]
fn every_mirrored_struct_field_has_a_matching_ts_field() {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let types_ts = fs::read_to_string(Path::new(manifest_dir).join("../src/lib/types.ts")).expect("read types.ts");

    let mut failures = Vec::new();

    let types = serialize_types();

    for &struct_name in MIRRORED_STRUCTS {
        let file = types.get(struct_name).unwrap_or_else(|| {
            panic!("no `Serialize`-deriving `{struct_name}` found anywhere in src/ — renamed, removed, or its derive dropped")
        });
        let rust_source = fs::read_to_string(Path::new(manifest_dir).join("src").join(file))
            .unwrap_or_else(|e| panic!("read src/{file}: {e}"));

        let rust_fields: BTreeSet<String> = rust_struct_fields(&rust_source, struct_name).iter().map(|f| to_camel_case(f)).collect();
        let ts_fields = ts_interface_fields(&types_ts, struct_name);

        let missing_in_ts: Vec<_> = rust_fields.difference(&ts_fields).collect();
        let missing_in_rust: Vec<_> = ts_fields.difference(&rust_fields).collect();

        if !missing_in_ts.is_empty() {
            failures.push(format!(
                "{struct_name} (src/{file}): field(s) {missing_in_ts:?} exist in Rust but not in types.ts's `interface {struct_name}` — the frontend can't read them"
            ));
        }
        if !missing_in_rust.is_empty() {
            failures.push(format!(
                "{struct_name}: types.ts declares field(s) {missing_in_rust:?} with no Rust counterpart in src/{file} — dead or renamed-away field"
            ));
        }
    }

    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}


#[test]
fn every_serialize_type_is_either_mirrored_or_explicitly_unmirrored() {
    let found = serialize_types();
    assert!(
        !found.is_empty(),
        "found no `#[derive(.., Serialize, ..)]` type at all in src/ — the scan broke, not the code"
    );

    let found_names: BTreeSet<String> = found.keys().cloned().collect();
    let classified: BTreeSet<String> = MIRRORED_STRUCTS
        .iter()
        .map(|s| (*s).to_string())
        .chain(UNMIRRORED_TYPES.iter().map(|(name, _)| (*name).to_string()))
        .collect();

    let unclassified: Vec<_> = found_names.difference(&classified).collect();
    assert!(
        unclassified.is_empty(),
        "Serialize-deriving type(s) {unclassified:?} in src/ are in neither table: add to MIRRORED_STRUCTS \
         (plus a matching `interface` in src/lib/types.ts) if the frontend reads them, or to UNMIRRORED_TYPES \
         with the reason they never cross to TS"
    );

    let stale: Vec<_> = classified.difference(&found_names).collect();
    assert!(
        stale.is_empty(),
        "MIRRORED_STRUCTS/UNMIRRORED_TYPES name(s) {stale:?} no longer exist as Serialize-deriving types in src/ \
         — renamed, removed, or derive dropped; drop or update the entry"
    );
}
