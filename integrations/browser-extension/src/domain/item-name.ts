// A vault item's name, as the popup may ask to fill it. The judge is Rust's `ItemName::parse_new`
// (crates/domain/src/vault.rs) once the native host reads the name; this is the same rule checked
// early. Both sides' tests read contracts/item-names.json (J-165).

/** At most this many characters (Unicode scalar values, as Rust's `chars().count()`). */
const MAX_CHARS = 64;

/**
 * One path segment: Rust `char::is_alphanumeric` is the Alphabetic property (which takes
 * Other_Alphabetic marks such as U+093F, unlike \p{L}) or a Numeric category (\p{N}).
 */
const SEGMENT = /^[\p{Alphabetic}\p{N}_-]+$/u;

/** 1–64 characters in non-empty segments joined by '/'. No '~', so never a draft (`github~draft-1`). */
export function isFillableItemName(value: string): boolean {
  const chars = [...value].length;
  return chars >= 1 && chars <= MAX_CHARS && value.split("/").every((segment) => SEGMENT.test(segment));
}
