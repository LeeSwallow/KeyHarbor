// What a drop onto a session card carries: a tab dragged from the browser arrives as a
// text/uri-list (or plain text). Only the first http(s) line counts; the app sends nothing else.

export function droppedUrl(text: string): string | null {
  for (const raw of text.split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith("#")) continue;
    return /^https?:\/\//i.test(line) ? line : null;
  }
  return null;
}
