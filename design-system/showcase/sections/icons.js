// Every icon in design-system/icons. design-check keeps this list and the folder in step.
import { icon, each } from "../html.js";

export const ICONS = ["app-window", "bot", "braces", "check", "chevron-down", "circle-play", "circle-stop", "clock", "copy", "credit-card", "download", "eye-off", "eye", "file-pen-line", "globe", "hand", "house", "id-card", "inbox", "key-round", "key-square", "key", "life-buoy", "lock", "map-pin", "plug-zap", "plus", "refresh-ccw-dot", "scroll-text", "search", "settings", "shield-check", "shield-x", "square-terminal", "trash-2", "triangle-alert", "upload", "user-round", "wallet", "wand-sparkles", "x"];

export const icons = {
  id: "icons",
  title: ["아이콘", "Icons"],
  lead: ["Lucide(ISC), 24px 격자, 2px 선. CSS mask로 currentColor를 칠합니다.", "Lucide (ISC), 24px grid, 2px stroke. Drawn in currentColor with a CSS mask."],
  body: () => `<div class="icons" id="icons">${each(ICONS, n => `<figure>${icon(n, "lg")}<figcaption>${n}</figcaption></figure>`)}</div>`,
};
