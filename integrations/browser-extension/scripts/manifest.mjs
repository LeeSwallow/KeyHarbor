// Merges the base manifest with a browser overlay. Objects merge key by key, arrays are joined
// without duplicates, anything else in the overlay wins. Pure.

const isObject = (x) => typeof x === "object" && x !== null && !Array.isArray(x);

export function mergeManifest(base, overlay) {
  const out = { ...base };
  for (const [key, value] of Object.entries(overlay)) {
    const current = out[key];
    if (Array.isArray(current) && Array.isArray(value)) out[key] = [...new Set([...current, ...value])];
    else if (isObject(current) && isObject(value)) out[key] = mergeManifest(current, value);
    else out[key] = value;
  }
  return out;
}
