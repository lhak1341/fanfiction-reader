import { describe, expect, it } from "vitest";
import { clampStep } from "./numeric";

describe("clampStep", () => {
  it("steps within range", () => {
    expect(clampStep(18, 1, { min: 14, max: 28 })).toBe(19);
    expect(clampStep(18, -1, { min: 14, max: 28 })).toBe(17);
  });

  it("clamps at the lower bound", () => {
    expect(clampStep(14, -1, { min: 14, max: 28 })).toBe(14);
  });

  it("clamps at the upper bound", () => {
    expect(clampStep(28, 1, { min: 14, max: 28 })).toBe(28);
  });

  it("rounds to the given decimals, avoiding float drift", () => {
    expect(clampStep(1.75, 0.1, { min: 1.2, max: 2.4, decimals: 2 })).toBe(1.85);
    expect(clampStep(1.2, -0.1, { min: 1.2, max: 2.4, decimals: 2 })).toBe(1.2);
  });

  it("rounds to 3 decimals for letter spacing", () => {
    expect(clampStep(0, 0.01, { min: -0.05, max: 0.2, decimals: 3 })).toBe(0.01);
    expect(clampStep(0.2, 0.01, { min: -0.05, max: 0.2, decimals: 3 })).toBe(0.2);
  });
});
