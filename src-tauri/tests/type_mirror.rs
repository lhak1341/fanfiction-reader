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
//! risk shape is different (see `bookmarked_only`'s doc comment,
//! `queries.rs`, for the one real bug this distinction already caught — a
//! dead TS field, not a name mismatch, which no field-name-diffing test
//! could have caught).
//!
//! TS interface names are required to match their Rust struct name exactly
//! (`ChapterFetched` (Rust) <-> `ChapterFetched` (TS), not `ChapterFetchedEvent`)
//! so pairing them up here needs no separate Rust-name-to-TS-name mapping —
//! one more hand-maintained list that could itself drift.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// (Rust source file relative to `src/`, struct name) for every `Serialize`
/// struct whose shape the frontend actually reads. Deliberately excludes
/// `UnmatchedFetchError` (`lib.rs`) — the `FetchTarget::Unknown` fallback has
/// no identity fields by design (see its doc comment) and no TS-side
/// counterpart to pair it with.
const MIRRORED_STRUCTS: &[(&str, &str)] = &[
    ("queries.rs", "StoryRow"),
    ("queries.rs", "ChapterListItem"),
    ("queries.rs", "ChapterContent"),
    ("db.rs", "FandomIndexEntry"),
    ("lib.rs", "ChapterFetched"),
    ("lib.rs", "BrowseFetched"),
    ("lib.rs", "FandomIndexFetched"),
    ("lib.rs", "ChapterFetchError"),
    ("lib.rs", "BrowseFetchError"),
    ("lib.rs", "FandomIndexFetchError"),
];

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

    for &(file, struct_name) in MIRRORED_STRUCTS {
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
