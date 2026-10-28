// The header switchers: theme, density, pattern and language. Each writes one attribute on <html>.

function segmented(el, apply) {
  el.addEventListener("click", e => {
    const b = e.target.closest("button");
    if (!b) return;
    el.querySelectorAll("button").forEach(x => x.classList.toggle("is-on", x === b));
    apply(b.dataset.v);
  });
}

function setLanguage(doc, lang) {
  doc.documentElement.lang = lang;
  doc.querySelectorAll("[data-en]").forEach(el => {
    if (!el.dataset.ko) el.dataset.ko = el.textContent;
    el.textContent = lang === "en" ? el.dataset.en : el.dataset.ko;
  });
  doc.querySelectorAll("[data-en-ph]").forEach(el => {
    if (!el.dataset.koPh) el.dataset.koPh = el.placeholder;
    el.placeholder = lang === "en" ? el.dataset.enPh : el.dataset.koPh;
  });
}

const SWITCHES = {
  theme: (doc, v) => { doc.documentElement.dataset.theme = v; },
  density: (doc, v) => { doc.documentElement.dataset.density = v; },
  pattern: (doc, v) => { doc.documentElement.dataset.pattern = v; },
  lang: setLanguage,
};

/** Wires the switchers, then applies ?theme=&lang=&pattern=&density= from the URL. */
export function installControls(doc, search) {
  for (const [id, apply] of Object.entries(SWITCHES)) {
    segmented(doc.getElementById(id), v => apply(doc, v));
  }
  const q = new URLSearchParams(search);
  for (const id of Object.keys(SWITCHES)) {
    const v = q.get(id);
    if (v) doc.querySelector(`#${id} [data-v="${v}"]`)?.click();
  }
}
