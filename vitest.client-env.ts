// A plain-Node vitest environment that Vite treats as `client`, so
// vite-plugin-svelte compiles runes for the browser. Under the built-in
// `node` environment modules are transformed for SSR, where `$effect` is a
// no-op — that is what kept *Screen reactive wiring out of unit tests.
// Still Node: no DOM, `localStorage` stays undefined (docs/TESTING.md).
export default {
  name: "node-client",
  viteEnvironment: "client",
  async setup() {
    return { teardown() {} };
  }
};
