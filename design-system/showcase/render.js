// Renders section definitions into the page. A definition is data:
//   { id, title: [ko, en], lead?: [ko, en], body?: () => html, parts?: [{ title: [ko, en] | string, body: () => html }] }
// Adding a section to the showcase means adding one definition to sections/index.js.

const heading = (tag, cls, title) =>
  Array.isArray(title)
    ? `<${tag}${cls} data-en="${title[1]}">${title[0]}</${tag}>`
    : `<${tag}${cls}>${title}</${tag}>`;

export function renderSection(def) {
  const lead = def.lead ? heading("p", "", def.lead) : "";
  const body = def.body ? def.body() : "";
  const parts = (def.parts ?? []).map(p => heading("h3", ' class="sub"', p.title) + p.body()).join("");
  return `<section id="${def.id}">${heading("h2", "", def.title)}${lead}${body}${parts}</section>`;
}

export function renderSections(defs, root) {
  root.innerHTML = defs.map(renderSection).join("");
}
