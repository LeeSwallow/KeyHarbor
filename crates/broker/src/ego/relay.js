// KeyHarbor's ego lite relay (ADR-0018). Runs inside `ego-browser nodejs -e`, one per session.
//
// It decides nothing. It connects to the supervisor's socket and speaks the CDP pipe's frames
// (one JSON message per NUL). Commands with a sessionId go to that ego Page (`page.cdp`);
// browser-level `Target.*` calls are answered from the TaskSpace's Pages, so the supervisor's
// CDP client works as it does over Chrome's pipe. Page events are taken from `page.events()`
// every `pollMs` and pushed. The supervisor fills CONFIG in.
const CONFIG = __CONFIG__;

const net = await import("node:net");

const task = await taskSpace(CONFIG.space);
/** label -> Page, the Pages this relay drives. */
const pages = new Map();
let autoAttach = false;
let closing = false;

const socket = net.createConnection(CONFIG.socket);
await new Promise((ready, fail) => {
  socket.once("connect", ready);
  socket.once("error", fail);
});

function send(message) {
  if (!socket.destroyed) socket.write(JSON.stringify(message) + "\0");
}

async function info(label, page) {
  const safe = async (read) => {
    try {
      return await read();
    } catch {
      return "";
    }
  };
  return {
    targetId: page.targetId,
    type: "page",
    title: await safe(() => page.title()),
    url: await safe(() => page.url()),
    attached: true,
    browserContextId: "ego",
  };
}

async function announce(label, page) {
  if (!autoAttach) return;
  send({
    method: "Target.attachedToTarget",
    params: { sessionId: label, targetInfo: await info(label, page), waitingForDebugger: false },
  });
}

function track(page) {
  if (pages.has(page.label)) return false;
  pages.set(page.label, page);
  return true;
}

function forget(label) {
  const page = pages.get(label);
  if (!page) return;
  pages.delete(label);
  send({ method: "Target.detachedFromTarget", params: { sessionId: label } });
  send({ method: "Target.targetDestroyed", params: { targetId: page.targetId } });
}

function byTarget(targetId) {
  for (const [label, page] of pages) if (page.targetId === targetId) return [label, page];
  throw new Error("No target with given id found");
}

track(task.page("p1"));

/** Answers a browser-level command (no sessionId). */
async function browserCommand(method, params) {
  switch (method) {
    case "Target.setAutoAttach":
      autoAttach = Boolean(params.autoAttach);
      for (const [label, page] of pages) await announce(label, page);
      return {};
    case "Target.setDiscoverTargets":
      return {};
    case "Target.getTargets":
      return { targetInfos: await Promise.all([...pages].map(([l, p]) => info(l, p))) };
    case "Target.attachToTarget":
      return { sessionId: byTarget(params.targetId)[0] };
    case "Target.activateTarget":
      byTarget(params.targetId);
      try {
        await task.cdp("Target.activateTarget", params);
      } catch {}
      return {};
    case "Target.createTarget": {
      const page = await task.newPage();
      track(page);
      await announce(page.label, page);
      if (params.url && params.url !== "about:blank") {
        await page.cdp("Page.navigate", { url: params.url });
      }
      return { targetId: page.targetId };
    }
    case "Browser.close":
      setTimeout(shutdown, 0);
      return {};
    default:
      return await task.cdp(method, params);
  }
}

/** Answers a command for one Page. */
async function pageCommand(label, method, params) {
  const page = pages.get(label);
  if (!page) throw new Error("Session with given id not found");
  switch (method) {
    case "Runtime.runIfWaitingForDebugger":
      return {};
    case "Page.close":
      await page.close();
      forget(label);
      return {};
    default:
      return await page.cdp(method, params);
  }
}

async function handle(message) {
  const { id, method, params = {}, sessionId } = message;
  try {
    const result = sessionId
      ? await pageCommand(sessionId, method, params)
      : await browserCommand(method, params);
    send({ id, sessionId, result: result ?? {} });
  } catch (error) {
    send({ id, sessionId, error: { code: -32000, message: String(error?.message ?? error) } });
  }
}

let buffered = "";
socket.setEncoding("utf8");
socket.on("data", (chunk) => {
  buffered += chunk;
  let end;
  while ((end = buffered.indexOf("\0")) >= 0) {
    const frame = buffered.slice(0, end);
    buffered = buffered.slice(end + 1);
    if (frame) handle(JSON.parse(frame));
  }
});
socket.on("close", shutdown);
socket.on("error", shutdown);

/** Pushes what each Page reported since the last look. */
async function pump() {
  for (const [label, page] of pages) {
    let events;
    try {
      events = await page.events();
    } catch {
      continue;
    }
    for (const event of events) {
      // ego's own child sessions are not ours to route; the supervisor sees Pages only.
      if (!event?.method || event.method.startsWith("Target.")) continue;
      send({ method: event.method, params: event.params ?? {}, sessionId: label });
    }
  }
}

/** Picks up Pages the session's pages opened (popups) and notices closed ones. */
async function reconcile() {
  let tabs;
  try {
    tabs = await task.tabs();
  } catch {
    return;
  }
  const open = new Set(tabs.map((tab) => tab.targetId));
  for (const [label, page] of [...pages]) if (!open.has(page.targetId)) forget(label);
  for (const tab of tabs) {
    if (tab.label || tab.openedBy === "user" || tab.openedBy === "unknown") continue;
    try {
      const page = await task.adopt(tab.page);
      if (track(page)) await announce(page.label, page);
    } catch {}
  }
}

async function shutdown() {
  if (closing) return;
  closing = true;
  try {
    await task.finish({ keep: [] });
  } catch {}
  socket.destroy();
  process.exit(0);
}

let ticks = 0;
while (!closing) {
  await pump();
  if (++ticks % CONFIG.reconcileEvery === 0) await reconcile();
  await new Promise((wake) => setTimeout(wake, CONFIG.pollMs));
}
