<script lang="ts">
  import { clampStep } from "$lib/numeric";

  let {
    label,
    value,
    min,
    max,
    step,
    decimals = 0,
    unit = "",
    onchange,
  }: {
    label: string;
    value: number;
    min: number;
    max: number;
    step: number;
    decimals?: number;
    unit?: string;
    onchange: (v: number) => void;
  } = $props();
</script>

<span class="font-label">{label}</span>
<div class="stepper">
  <button
    onclick={() => onchange(clampStep(value, -step, { min, max, decimals }))}
    aria-label="Decrease {label}">−</button
  >
  <span class="tnum">{value.toFixed(decimals)}{unit}</span>
  <button
    onclick={() => onchange(clampStep(value, step, { min, max, decimals }))}
    aria-label="Increase {label}">+</button
  >
</div>

<style>
  .font-label {
    font-size: 0.72rem;
    color: var(--ink-soft);
    white-space: nowrap;
  }
  .stepper {
    display: flex;
    width: 100%;
    align-items: center;
    border: 1px solid var(--line);
    border-radius: 6px;
    overflow: hidden;
  }
  .stepper button {
    flex-shrink: 0;
    background: var(--paper);
    border: none;
    color: var(--ink-soft);
    padding: 0.3rem 0.5rem;
    font-size: 0.72rem;
    border-right: 1px solid var(--line);
  }
  .stepper button:last-child {
    border-right: none;
  }
  .tnum {
    flex: 1;
    text-align: center;
    padding: 0 0.4rem;
    font-size: 0.76rem;
    color: var(--ink);
  }
</style>
