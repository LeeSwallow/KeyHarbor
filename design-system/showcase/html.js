// Small HTML builders shared by the section definitions. Pure: strings in, strings out.

/** An icon from design-system/icons, drawn by the kh-i component. */
export const icon = (name, cls = "", style = "") =>
  `<span class="kh-i${cls ? " " + cls : ""}" style="mask-image:url(icons/${name}.svg)${style ? ";" + style : ""}"></span>`;

/** Korean text with its English twin; the language switch swaps them. */
export const t = (ko, en) => `<t data-en="${en}">${ko}</t>`;

/** Repeats a template over a list and joins the result. */
export const each = (list, fn) => list.map(fn).join("");
