// A removal that waits before it is carried out (09 §9.4 "실행 취소 10초"): items, and
// sessions the same way (#123). One frame for both (#182).

import type { Api } from "../api.js";
import { type State, undoRemove, undoRemoveSession } from "../state.js";
import { type Win, load } from "./window.js";

/// How long a removal can be taken back. Quitting before it runs out keeps what was removed:
/// the safe way to fail.
const UNDO_MS = 10_000;

/// One kind of removal: what waits in the state, the state with nothing waiting, and the call
/// that carries it out.
export interface Removal<T> {
  waiting(s: State): T | null;
  forget(s: State): State;
  remove(api: Api, waiting: T): Promise<unknown>;
}

export const ITEM_REMOVAL: Removal<string> = {
  waiting: (s) => s.removing,
  forget: undoRemove,
  remove: (api, name) => api.removeItem(name),
};

export const SESSION_REMOVAL: Removal<{ id: string; name: string }> = {
  waiting: (s) => s.removingSession,
  forget: undoRemoveSession,
  remove: (api, session) => api.removeSession(session.id),
};

export interface Undo {
  /// Carries out a removal still waiting, then marks a new one (`mark`) and starts its time.
  start(mark: (s: State) => State): void;
  /// Takes the waiting removal back.
  undo(): void;
}

export function undoTimer<T>(w: Win, removal: Removal<T>): Undo {
  let timer: ReturnType<typeof setTimeout> | undefined;
  const commit = () => {
    clearTimeout(timer);
    const waiting = removal.waiting(w.state);
    if (!waiting) return;
    w.set(removal.forget(w.state));
    void w.guard(() => removal.remove(w.api, waiting).then(() => load(w)));
  };
  return {
    start: (mark) => {
      // Another removal still waiting is carried out now; this one waits its own time.
      commit();
      w.set(mark(w.state));
      timer = setTimeout(commit, UNDO_MS);
    },
    undo: () => {
      clearTimeout(timer);
      w.set(removal.forget(w.state));
    },
  };
}
