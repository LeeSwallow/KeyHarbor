// PageForm: runs a small function in the tab's top frame (the `scripting` permission and the
// tab access `activeTab` gives on the person's click). Frames of other sites are never touched.

import type { LoginValues } from "../../domain/login.js";
import type { PageForm } from "../../ports/index.js";
import type { WebExtApi } from "./api.js";

/**
 * Runs inside the page. Must be self-contained: it is serialised, not closed over. Fills the
 * first visible password field and the text or email field before it, as if typed, but only
 * while the page is still on `origin`.
 */
export function fillPageLogin(origin: string, username: string | null, password: string): "filled" | "no_form" | "moved" {
  if (location.origin !== origin) return "moved";
  const visible = (el: HTMLInputElement) => !el.disabled && el.offsetParent !== null;
  const pw = Array.from(document.querySelectorAll<HTMLInputElement>('input[type="password"]')).find(visible);
  if (!pw) return "no_form";
  const set = (el: HTMLInputElement, value: string) => {
    const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")?.set;
    setter?.call(el, value);
    el.dispatchEvent(new Event("input", { bubbles: true }));
    el.dispatchEvent(new Event("change", { bubbles: true }));
  };
  if (username !== null) {
    const scope = pw.form ?? document;
    const fields = Array.from(scope.querySelectorAll<HTMLInputElement>('input[type="text"], input[type="email"], input:not([type])')).filter(visible);
    const before = fields.filter((f) => f.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING);
    const user = before[before.length - 1];
    if (user) set(user, username);
  }
  set(pw, password);
  return "filled";
}

/** Runs inside the page. The filled password field and the username before it, or null. */
export function readPageLogin(origin: string): { username: string | null; password: string } | null {
  if (location.origin !== origin) return null;
  const pw = Array.from(document.querySelectorAll<HTMLInputElement>('input[type="password"]')).find((el) => el.value !== "");
  if (!pw) return null;
  const scope = pw.form ?? document;
  const fields = Array.from(scope.querySelectorAll<HTMLInputElement>('input[type="text"], input[type="email"], input:not([type])'));
  const before = fields.filter((f) => f.value !== "" && f.compareDocumentPosition(pw) & Node.DOCUMENT_POSITION_FOLLOWING);
  return { username: before[before.length - 1]?.value ?? null, password: pw.value };
}

export function webextPageForm(api: WebExtApi): PageForm {
  return {
    async fill(tabId, origin, login) {
      const [frame] = await api.scripting.executeScript({ target: { tabId }, func: fillPageLogin, args: [origin, login.username ?? null, login.password] });
      const result = frame?.result;
      return result === "filled" || result === "moved" ? result : "no_form";
    },
    async read(tabId, origin) {
      const [frame] = await api.scripting.executeScript({ target: { tabId }, func: readPageLogin, args: [origin] });
      const result = frame?.result as { username: unknown; password: unknown } | null | undefined;
      if (!result || typeof result.password !== "string" || result.password === "") return null;
      const values: LoginValues = { password: result.password };
      if (typeof result.username === "string" && result.username !== "") values.username = result.username;
      return values;
    },
  };
}
