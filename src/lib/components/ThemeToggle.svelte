<script lang="ts">
  import { onMount } from "svelte";
  import { Moon, Monitor, Sun } from "lucide-svelte";
  import { applyTheme, getStoredTheme, type ThemePref } from "$lib/theme";

  let pref = $state<ThemePref>("system");

  onMount(() => {
    pref = getStoredTheme();
  });

  function setPref(p: ThemePref) {
    pref = p;
    applyTheme(p);
  }
</script>

<div class="theme-toggle" role="group" aria-label="Theme">
  <button class:active={pref === "light"} onclick={() => setPref("light")} title="Light">
    <Sun size={14} />
  </button>
  <button class:active={pref === "system"} onclick={() => setPref("system")} title="Match system">
    <Monitor size={14} />
  </button>
  <button class:active={pref === "dark"} onclick={() => setPref("dark")} title="Dark">
    <Moon size={14} />
  </button>
</div>

<style>
  .theme-toggle {
    display: inline-flex;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .theme-toggle button {
    background: var(--paper);
    border: none;
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    color: var(--ink-soft);
    border-right: 1px solid var(--line);
  }
  .theme-toggle button:last-child {
    border-right: none;
  }
  .theme-toggle button.active {
    background: var(--accent-wash);
    color: var(--accent);
  }
</style>
