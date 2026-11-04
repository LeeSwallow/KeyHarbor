// The window as the event handlers see it: the current state, how to change it, and the
// moves several handlers share (loading, sending a change that may ask, the palette).

import type { Api, Outcome } from "../api.js";
import { type State, asking, closeForm, closePalette, editNew, go, select } from "../state.js";
import { AUDIT_SHOWN } from "../view.js";
import { type Run, paletteCommands } from "../views/palette.js";

export interface Win {
  readonly root: HTMLElement;
  readonly api: Api;
  /// Always the current state: read it after an `await`, not before.
  readonly state: State;
  set(next: State): void;
  /// Runs `work`; a failure is shown in the window instead of thrown.
  guard(work: () => Promise<unknown>): Promise<void>;
  /// Whether the person is typing in the item search, so a re-render gives it focus back.
  typing: boolean;
}

/// Re-rendering replaces the palette and search inputs; keep typing where it was.
export function refocus(root: HTMLElement, typing: boolean): void {
  const input = root.querySelector<HTMLInputElement>("[data-palette]") ?? (typing ? root.querySelector<HTMLInputElement>("[data-search]") : null);
  if (input) {
    input.focus();
    input.setSelectionRange(input.value.length, input.value.length);
  }
}

export const load = (w: Win) =>
  w.guard(async () => {
    const { api } = w;
    const [overview, inbox, sessions, items, audit, handoffs, settings, policies, kinds, templates] = await Promise.all([
      api.overview(), api.inbox(), api.sessions(), api.items(), api.audit(AUDIT_SHOWN),
      api.handoffs(), api.settings(), api.policies(), api.kinds(), api.templates(),
    ]);
    w.set({ ...w.state, overview, inbox, sessions, items, audit, handoffs, settings, policies, kinds, templates });
  });

/// After a request is answered: the secure input window closes, the main window reloads.
export const finishRequest = (w: Win) => (w.state.alone ? w.api.closeRequestWindow() : (w.set(closeForm(w.state)), load(w)));

/// Sends a change; if it needs a person, asks first and resends it confirmed on "확인".
export const change = (w: Win, send: (confirmed: boolean) => Promise<Outcome>) =>
  w.guard(async () => {
    const outcome = await send(false);
    if (outcome.outcome === "ask") {
      w.set(asking(w.state, { question: outcome.question, retry: () => send(true).then(() => load(w)) }));
    } else {
      await load(w);
    }
  });

/// The palette commands for what is typed in it.
export const matches = (w: Win) => (w.state.palette ? paletteCommands(w.state, w.state.palette.query) : []);

/// Runs a palette command; the palette closes first.
export function runCommand(w: Win, r: Run): void {
  const closed = closePalette(w.state);
  if (r.kind === "go") w.set(go(closed, r.view));
  if (r.kind === "select") w.set(select(go(closed, "items"), r.name));
  if (r.kind === "new-item") w.set(editNew(go(closed, "items")));
}
