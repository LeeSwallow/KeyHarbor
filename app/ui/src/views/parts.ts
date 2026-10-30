// The design system's parts as the window uses them: buttons with a variant, icons, a kind's
// glyph, badges, and the card row every list is made of. Screens build from these, so a list
// looks the same on every screen.

import { esc } from "../html.js";
import { kindLook } from "../words.js";

export type Variant = "primary" | "secondary" | "ghost" | "danger";

/// A button; `attrs` is trusted markup (`data-action` and escaped data attributes).
export const btn = (label: string, attrs: string, variant: Variant = "secondary", size = "sm") =>
  `<button type="button" class="kh-btn ${variant} ${size}" ${attrs}>${esc(label)}</button>`;

/// An icon from design-system/icons, through the `.i-<name>` classes in app.css (the CSP
/// forbids the inline style the design system's own example uses).
export const icon = (name: string, size = "sm") => `<span class="kh-i ${size} i-${name}" aria-hidden="true"></span>`;

export const glyph = (kind: string) => {
  const look = kindLook[kind] ?? { domain: "session", icon: "key" };
  return `<span class="glyph ${look.domain}">${icon(look.icon)}</span>`;
};

export const badge = (text: string, tone: string) => `<span class="kh-badge ${tone}">${esc(text)}</span>`;

/// One entry of a list: an optional glyph, a title, a line under it, and its actions.
export function card(p: { glyph?: string; title: string; sub?: string; actions?: string; extra?: string }): string {
  return `<div class="kh-row card">${p.glyph ?? ""}<div class="main"><div class="title">${p.title}</div>${
    p.sub ? `<div class="sub">${p.sub}</div>` : ""
  }${p.extra ?? ""}</div>${p.actions ? `<div class="actions">${p.actions}</div>` : ""}</div>`;
}

/// A screen's heading with an optional line under it.
export const heading = (title: string, sub = "") =>
  `<header class="screen-head"><h2>${esc(title)}</h2>${sub ? `<p class="kh-help">${esc(sub)}</p>` : ""}</header>`;

export const empty = (text: string) => `<div class="kh-empty">${esc(text)}</div>`;
