export type ViewMode = "list" | "card";

const STORAGE_KEY = "ffn-view-mode";

export function getViewMode(): ViewMode {
  if (typeof localStorage === "undefined") return "list";
  return localStorage.getItem(STORAGE_KEY) === "card" ? "card" : "list";
}

export function saveViewMode(mode: ViewMode) {
  if (typeof localStorage === "undefined") return;
  localStorage.setItem(STORAGE_KEY, mode);
}
