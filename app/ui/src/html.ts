// Escaping for every text the window shows: item names, agents' reasons, origins, values.

export const esc = (text: string): string => text.replace(/[&<>"']/g, (c) => `&#${c.charCodeAt(0)};`);
