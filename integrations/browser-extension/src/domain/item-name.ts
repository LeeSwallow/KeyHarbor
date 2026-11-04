// A vault item's name, as the popup may ask to fill it. The judge is Rust's `ItemName::parse_new`
// (crates/domain/src/vault.rs) once the native host reads the name; this is the early check.
// It is looser than Rust in two ways, left as they were when the rule moved here (#149): it
// accepts empty segments ("a//b", "/a"), and it uses \p{L} where Rust's `is_alphabetic` also
// takes combining marks. No contracts/ fixture holds item names yet.

/** 1–64 letters, digits, '-', '_', '/'. No '~', so never a draft (`github~draft-1`). */
const FILLABLE_ITEM_NAME = /^[\p{L}\p{N}_\-/]{1,64}$/u;

export const isFillableItemName = (value: string): boolean => FILLABLE_ITEM_NAME.test(value);
