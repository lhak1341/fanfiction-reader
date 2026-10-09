#!/usr/bin/env node
/**
 * Lint gate with a ratcheting warning ceiling.
 *
 * Every meaningful rule in eslint.config.js is warn-severity, so a plain `eslint .` exits 0
 * no matter how many findings there are — a CI lint step wired to it can never fail. This
 * wrapper turns the warning count into an enforced budget:
 *
 *   - count > baseline  -> fail; the change added lint warnings
 *   - count < baseline  -> fail; lower the baseline (keeps the ceiling from going stale)
 *   - count === baseline -> pass
 *
 * Run with --update to write the current count as the new baseline. Run with --ratchet-down
 * (the pre-commit hook does) to lower the baseline on a drop instead of failing; an increase
 * and any error still fail.
 */
import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const baselinePath = path.join(root, '.lint-baseline.json');

let raw;
try {
  raw = execFileSync(path.join(root, 'node_modules', '.bin', 'eslint'), ['.', '-f', 'json'], {
    cwd: root,
    encoding: 'utf8',
    maxBuffer: 64 * 1024 * 1024,
    stdio: ['ignore', 'pipe', 'inherit'],
  });
} catch (err) {
  // eslint exits non-zero when there are errors; the JSON report is still on stdout.
  if (!err.stdout) throw err;
  raw = err.stdout;
}

const results = JSON.parse(raw);
const errors = results.reduce((n, f) => n + f.errorCount, 0);
const warnings = results.reduce((n, f) => n + f.warningCount, 0);

if (process.argv.includes('--update')) {
  writeFileSync(baselinePath, `${JSON.stringify({ maxWarnings: warnings }, null, 2)}\n`);
  console.log(`lint baseline updated: ${warnings} warnings`);
  process.exit(0);
}

const { maxWarnings } = JSON.parse(readFileSync(baselinePath, 'utf8'));

if (errors > 0) {
  console.error(`lint: ${errors} error(s). Run \`bun run lint:full\` for details.`);
  process.exit(1);
}

if (warnings > maxWarnings) {
  console.error(
    `lint: ${warnings} warnings exceeds the baseline of ${maxWarnings}.\n` +
      `This change added ${warnings - maxWarnings}. Run \`bun run lint:full\` to see them.`
  );
  process.exit(1);
}

if (warnings < maxWarnings && process.argv.includes('--ratchet-down')) {
  writeFileSync(baselinePath, `${JSON.stringify({ maxWarnings: warnings }, null, 2)}\n`);
  console.log(`lint: ${warnings} warnings is below the baseline of ${maxWarnings}; lowered it.`);
  process.exit(0);
}

if (warnings < maxWarnings) {
  console.error(
    `lint: ${warnings} warnings is below the baseline of ${maxWarnings} — nice.\n` +
      `Lock it in with \`bun run lint:baseline\` so the ceiling cannot drift back up.`
  );
  process.exit(1);
}

console.log(`lint: ${warnings} warnings, at baseline. 0 errors.`);
