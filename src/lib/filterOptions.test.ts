import { describe, expect, it } from "vitest";
import {
  SEARCH_STATUSES,
  SORT_LABELS,
  SORT_OPTIONS,
  SOURCE_LABELS,
  STATUS_LABELS,
} from "./filterOptions";

describe("filterOptions", () => {
  it("lists every sort in the order the <select> shows", () => {
    expect(SORT_OPTIONS.map(([value]) => value)).toEqual(["favorites", "fetched", "words"]);
    expect(SORT_OPTIONS.map(([value]) => value)).toEqual(Object.keys(SORT_LABELS));
  });

  it("labels both Sources and both statuses", () => {
    expect(SOURCE_LABELS).toEqual({ ffn: "FFN", ao3: "AO3" });
    expect(STATUS_LABELS).toEqual({ complete: "Done", in_progress: "Ongoing" });
  });

  it("offers a new FFN search only Done (its URL can't encode in_progress)", () => {
    expect(SEARCH_STATUSES.ffn).toEqual(["complete"]);
    expect(SEARCH_STATUSES.ao3).toEqual(["complete", "in_progress"]);
  });
});
