// One function per kind of event the window listens for. Each checks what the event came from
// (`instanceof`) instead of assuming it, and hands the work to a handler table or a state move.

import { closePalette, edit, fail, go, openPalette, query, search } from "../state.js";
import { droppedUrl } from "../drop.js";
import { handOffWord } from "../words.js";
import { type Actions, dispatch } from "./actions.js";
import { type Forms, formKind } from "./forms.js";
import { type Win, load, matches, runCommand } from "./window.js";

/// The nearest element from `target` up that matches `selector`, if it is an HTML element.
function closestFrom(target: EventTarget | null, selector: string): HTMLElement | null {
  const found = target instanceof Element ? target.closest(selector) : null;
  return found instanceof HTMLElement ? found : null;
}

export function listenClicks(w: Win, actions: Actions): void {
  w.root.addEventListener("click", (event) => {
    w.typing = false;
    const button = closestFrom(event.target, "[data-action]");
    if (button) dispatch(actions, button);
  });
}

// A tab dragged from the browser onto a session card (spec 2026-10-31): the extension asks the
// person in their browser, so the answer may take a while.
export function listenDrops(w: Win): void {
  w.root.addEventListener("dragover", (event) => {
    if (closestFrom(event.target, "[data-session-drop]")) event.preventDefault();
  });
  w.root.addEventListener("drop", (event) => {
    const card = closestFrom(event.target, "[data-session-drop]");
    if (!card) return;
    event.preventDefault();
    const session = card.dataset.sessionDrop ?? "";
    const url = droppedUrl(event.dataTransfer?.getData("text/uri-list") || event.dataTransfer?.getData("text/plain") || "");
    if (!url) return w.set(fail(w.state, "브라우저의 탭이나 주소를 끌어다 놓으세요"));
    w.set({ ...w.state, notice: `${session} 세션에 넘길지 브라우저에서 확인하세요…`, error: null });
    void w.guard(() => w.api.requestSite(session, url).then((answer) => w.set({ ...w.state, notice: handOffWord(answer, session) })).then(() => load(w)));
  });
}

export function listenInput(w: Win): void {
  w.root.addEventListener("input", (event) => {
    const input = event.target;
    if (!(input instanceof HTMLInputElement)) return;
    if (input.dataset.palette !== undefined) w.set(query(w.state, input.value));
    if (input.dataset.search !== undefined) {
      w.typing = true;
      w.set(search(w.state, { text: input.value }));
    }
  });
}

// 09 U6: ⌘K opens and closes the palette, ⌘F searches items, ⌘N starts a new item, Esc closes
// what is open.
export function listenKeys(w: Win): void {
  w.root.ownerDocument.addEventListener("keydown", (event) => {
    const command = event.metaKey || event.ctrlKey;
    if (command && event.key === "k") {
      event.preventDefault();
      w.set(w.state.palette ? closePalette(w.state) : openPalette(w.state));
    } else if (command && event.key === "f") {
      event.preventDefault();
      if (w.state.view !== "items") w.set(go(w.state, "items"));
      w.typing = true;
      w.root.querySelector<HTMLInputElement>("[data-search]")?.focus();
    } else if (command && event.key === "n") {
      event.preventDefault();
      runCommand(w, { kind: "new-item" });
    } else if (event.key === "Escape" && w.state.palette) {
      w.set(closePalette(w.state));
    } else if (event.key === "Enter" && w.state.palette) {
      event.preventDefault();
      const first = matches(w)[0];
      if (first) runCommand(w, first.run);
    }
  });
}

/// The new item form's kind picker.
export function listenPicks(w: Win): void {
  w.root.addEventListener("change", (event) => {
    const picker = event.target;
    if (picker instanceof HTMLSelectElement && picker.dataset.action === "pick-kind") {
      w.set(edit(w.state, { mode: "new", kind: picker.value }));
    }
  });
}

export function listenSubmits(w: Win, forms: Forms): void {
  w.root.addEventListener("submit", (event) => {
    event.preventDefault();
    const el = event.target;
    if (!(el instanceof HTMLFormElement)) return;
    const kind = formKind(el);
    if (kind) forms[kind](el, new FormData(el));
  });
}
