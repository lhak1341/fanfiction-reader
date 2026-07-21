export function clampStep(
  value: number,
  delta: number,
  { min, max, decimals = 0 }: { min: number; max: number; decimals?: number }
): number {
  const stepped = Math.min(max, Math.max(min, value + delta));
  const factor = 10 ** decimals;
  return Math.round(stepped * factor) / factor;
}
