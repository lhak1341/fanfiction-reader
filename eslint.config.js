import js from "@eslint/js";
import svelte from "eslint-plugin-svelte";
import prettier from "eslint-config-prettier";
import tseslint from "typescript-eslint";
import globals from "globals";

// Every rule is warn-severity so scripts/lint-ratchet.mjs can enforce a ceiling on the count.
const warnOnly = (configs) =>
  configs.map((c) =>
    c.rules
      ? {
          ...c,
          rules: Object.fromEntries(
            Object.entries(c.rules).map(([name, level]) => {
              const sev = Array.isArray(level) ? level[0] : level;
              if (sev === "off" || sev === 0 || sev === "warn" || sev === 1) return [name, level];
              return [name, Array.isArray(level) ? ["warn", ...level.slice(1)] : "warn"];
            })
          ),
        }
      : c
  );

export default tseslint.config(
  {
    ignores: [
      "build/",
      ".svelte-kit/",
      "src-tauri/target/",
      "src-tauri/gen/",
      "node_modules/",
      "graphify-out/",
      "coverage/",
      "temp/",
    ],
  },
  ...warnOnly([
    js.configs.recommended,
    ...tseslint.configs.recommended,
    ...svelte.configs.recommended,
  ]),
  prettier,
  ...svelte.configs.prettier,
  { languageOptions: { globals: { ...globals.browser, ...globals.node } } },
  {
    files: ["**/*.svelte", "**/*.svelte.js", "**/*.svelte.ts"],
    languageOptions: { parserOptions: { parser: tseslint.parser } },
  }
);
