// tauri.conf.json's `style-src` carries every style hash @lhak/style exports
// (NumberFlow's shadow-root styles, Motion's empty <style>), so the CSP and the
// package can't drift apart. After re-vendoring a new @lhak/style, set
// @number-flow/react to its NUMBER_FLOW_VERSION and paste STYLE_HASHES into
// `style-src` until this passes.
// Tauri never applies the CSP in `tauri dev`: check CSP changes in `tauri build`.
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { styles } from "@number-flow/react";
import { NUMBER_FLOW_STYLE_HASHES, NUMBER_FLOW_VERSION, STYLE_HASHES } from "@lhak/style/csp";
import { describe, expect, test } from "vitest";

const root = fileURLToPath(new URL("../../", import.meta.url));
const readJson = (file: string) => JSON.parse(readFileSync(join(root, file), "utf8"));

const csp: string = readJson("src-tauri/tauri.conf.json").app.security.csp ?? "";

/// The directive's sources.
const directive = (name: string): string[] =>
  (
    csp
      .split(";")
      .map((d) => d.trim().split(/\s+/))
      .find(([n]) => n === name) ?? []
  ).slice(1);

describe("CSP style-src", () => {
  const styleSrc = directive("style-src");

  test("keeps 'self' and never 'unsafe-inline'", () => {
    expect(styleSrc).toContain("'self'");
    expect(styleSrc).not.toContain("'unsafe-inline'");
  });

  test.each(STYLE_HASHES)("contains %s", (hash) => {
    expect(styleSrc).toContain(hash);
  });
});

// @lhak/style takes NumberFlow as a peer, so this app's copy is the one the hashes must fit.
describe("NumberFlow, as this app bundles it", () => {
  test("package.json pins the version the hashes were computed for", () => {
    expect(readJson("package.json").dependencies["@number-flow/react"]).toBe(NUMBER_FLOW_VERSION);
  });

  test("the installed version is that one", () => {
    // From node_modules: @number-flow/react's `exports` map hides its package.json.
    expect(readJson("node_modules/@number-flow/react/package.json").version).toBe(
      NUMBER_FLOW_VERSION
    );
  });

  test("its styles hash to the exported hashes", () => {
    const hash = (text: string) =>
      `'sha256-${createHash("sha256").update(text, "utf8").digest("base64")}'`;
    expect(styles.map(hash)).toEqual([...NUMBER_FLOW_STYLE_HASHES]);
  });
});
