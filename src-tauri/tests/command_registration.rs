//! Guard rail for architecture review candidate 4 (2026-07-19): a Tauri
//! command registered in `invoke_handler!` but missing from every
//! permission file's `commands.allow` compiles clean and fails only at
//! runtime with "not allowed" — no compile warning. Already hit once for
//! real (`search_ao3`/`fetch_ao3_page`, SPEC.md §8). This converts that
//! failure mode into a test-time one.
//!
//! A command only needs to be allowed by *some* permission file, not all of
//! them — they're scoped to different windows/capabilities (e.g.
//! `report_fetch_result` is only ever allowed for the `fetch-page` window's
//! capability, never the main window's `allow-main-commands`). So the real
//! invariant is: invoke_handler!'s command set == the union of
//! `commands.allow` across every `permissions/*.toml` file — not a
//! one-to-one match with any single file.
//!
//! Both sides are read from the actual source/config at test time (no
//! hand-maintained parallel list to drift), via small string scans rather
//! than a `toml` crate dependency — the permission files' `commands.allow =
//! [...]` shape is simple and fully within this project's control.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

/// Pulls the identifier list out of `tauri::generate_handler![ ... ]` in
/// `src/lib.rs`'s source text.
fn invoke_handler_commands() -> BTreeSet<String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let lib_rs = fs::read_to_string(Path::new(manifest_dir).join("src/lib.rs")).expect("read src/lib.rs");

    let start = lib_rs.find("generate_handler!").expect("no generate_handler! call found in src/lib.rs");
    let open = lib_rs[start..].find('[').expect("generate_handler! call has no [") + start;
    let close = lib_rs[open..].find(']').expect("generate_handler! call has no closing ]") + open;
    let inner = &lib_rs[open + 1..close];

    let names: Vec<&str> = inner.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();

    // A path-qualified entry (`fetch::report_fetch_result`) compiles and
    // registers fine, but `commands.allow` names commands bare, so it could
    // never match one — without this it surfaces as a confusing "missing
    // from permissions" for a command that is right there in the file.
    let qualified: Vec<&&str> = names.iter().filter(|n| n.contains("::")).collect();
    assert!(
        qualified.is_empty(),
        "generate_handler! lists path-qualified command(s) {qualified:?} — permissions/*.toml names commands bare, \
         so these can never match. `use module::fn_name;` at the top of src/lib.rs and list the bare name here."
    );
    assert!(!names.is_empty(), "generate_handler! parsed to an empty command list — the scan broke, not the code");

    names.into_iter().map(str::to_string).collect()
}

/// Pulls every `commands.allow = [...]` array out of every
/// `permissions/*.toml` file, unioned into one set.
fn permission_allow_commands() -> BTreeSet<String> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let permissions_dir = Path::new(manifest_dir).join("permissions");

    let mut allowed = BTreeSet::new();
    for entry in fs::read_dir(&permissions_dir).expect("read permissions/ dir") {
        let path = entry.expect("read dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("toml") {
            continue;
        }
        let contents = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        let mut rest = contents.as_str();
        while let Some(idx) = rest.find("commands.allow") {
            let after = &rest[idx..];
            let open = after.find('[').unwrap_or_else(|| panic!("commands.allow with no [ in {}", path.display()));
            let close =
                after[open..].find(']').unwrap_or_else(|| panic!("commands.allow with no closing ] in {}", path.display()));
            let inner = &after[open + 1..open + close];
            for name in inner.split(',') {
                let name = name.trim().trim_matches('"');
                if !name.is_empty() {
                    allowed.insert(name.to_string());
                }
            }
            rest = &after[open + close..];
        }
    }
    allowed
}

#[test]
fn every_invoke_handler_command_is_allowed_by_some_permission_file() {
    let registered = invoke_handler_commands();
    let allowed = permission_allow_commands();

    let missing: Vec<_> = registered.difference(&allowed).collect();
    assert!(
        missing.is_empty(),
        "commands registered in invoke_handler! but not allowed by any permissions/*.toml \
         (compiles clean, fails at runtime with \"not allowed\"): {missing:?}"
    );
}

#[test]
fn every_permission_allowed_command_is_actually_registered() {
    let registered = invoke_handler_commands();
    let allowed = permission_allow_commands();

    let stale: Vec<_> = allowed.difference(&registered).collect();
    assert!(stale.is_empty(), "permissions/*.toml allows commands that no longer exist in invoke_handler!: {stale:?}");
}
