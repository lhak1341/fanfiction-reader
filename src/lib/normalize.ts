const COMBINING_MARKS = /[\u0300-\u036f]/g;

/** Lowercases and strips diacritics so search is accent-insensitive (e.g. "poke" matches "Pok\u00e9"). */
export function fold(s: string): string {
  return s.toLowerCase().normalize("NFD").replace(COMBINING_MARKS, "");
}
