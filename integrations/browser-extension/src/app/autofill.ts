// Use cases: fill a saved login into the active tab, list what may fill it, and save what the
// person typed (phase 7, ADR-0017). The origin is always the tab's own; values pass straight
// from the host into the page, or from the page to the host, and are never kept or logged.

import type { LoginChoice, SaveOutcome } from "../domain/login.js";
import { type Result, err, ok } from "../domain/result.js";
import { scopeOf } from "../domain/scope.js";
import type { Ports } from "../ports/index.js";

async function originOf(ports: Ports, tabId: number): Promise<Result<string>> {
  const tab = await ports.tabs.find(tabId);
  if (!tab) return err("unsupported_page");
  const scope = scopeOf(tab.url);
  return scope.ok ? ok(scope.value.origin) : scope;
}

const viaHost = async <T>(call: () => Promise<Result<T>>): Promise<Result<T>> =>
  call().catch(() => err<T>("host_unavailable"));

export function makeListLogins(ports: Ports) {
  return async (tabId: number): Promise<Result<LoginChoice[]>> => {
    const origin = await originOf(ports, tabId);
    if (!origin.ok) return origin;
    return viaHost(() => ports.host.listLogins(origin.value));
  };
}

export function makeFillLogin(ports: Ports) {
  return async (tabId: number, item: string): Promise<Result<{ item: string }>> => {
    const origin = await originOf(ports, tabId);
    if (!origin.ok) return origin;
    const values = await viaHost(() => ports.host.fillLogin(origin.value, item));
    if (!values.ok) return values;
    // The page checks again that it is still on the origin: Touch ID takes a moment.
    const filled = await ports.page.fill(tabId, origin.value, values.value).catch(() => "no_form" as const);
    if (filled === "moved") return err("page_changed");
    if (filled === "no_form") return err("no_login_form");
    return ok({ item });
  };
}

export function makeSaveLogin(ports: Ports) {
  return async (tabId: number): Promise<Result<SaveOutcome>> => {
    const origin = await originOf(ports, tabId);
    if (!origin.ok) return origin;
    const login = await ports.page.read(tabId, origin.value).catch(() => null);
    if (!login) return err("no_login_form");
    return viaHost(() => ports.host.saveLogin(origin.value, login));
  };
}
