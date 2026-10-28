// Build targets. A new browser (e.g. Edge) is one entry here, an adapter folder under
// src/adapters/<browser>/ and an overlay in manifest/<browser>.json.

export const TARGETS = [
  {
    name: "chrome",
    entries: { background: "src/adapters/chrome/background.ts", popup: "src/adapters/chrome/popup.ts" },
    overlay: "manifest/chrome.json",
    esbuildTarget: "chrome120",
  },
  {
    name: "firefox",
    entries: { background: "src/adapters/firefox/background.ts", popup: "src/adapters/firefox/popup.ts" },
    overlay: "manifest/firefox.json",
    esbuildTarget: "firefox128",
  },
];
