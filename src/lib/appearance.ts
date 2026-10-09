// Settings → Appearance (GA-42): one font for the whole app, three text sizes (the chat, the interface, and tasks and
// docs), the theme and the density. Kept on this computer in localStorage, like the theme always was, and put on <html>
// before the first paint (main.tsx): data-font, data-theme and data-density, and the size steps that
// src/styles/appearance.css grows the text and the layout by. A change shows at once; Save settings isn't involved.
import { useEffect, useState } from "react";

export type FontKey = "atkinson" | "jetbrains-mono" | "inter" | "geist" | "hack";
export type Theme = "dark" | "light";
export type Density = "comfortable" | "compact";
export type Appearance = { font: FontKey; chat: number; ui: number; docs: number; theme: Theme; density: Density };

/** Each choice sets the text font and the code font (task IDs, code, run logs); appearance.css has their stacks. */
export const FONTS: { key: FontKey; name: string; note: string }[] = [
  { key: "atkinson", name: "Atkinson Hyperlegible", note: "The default: Atkinson Hyperlegible Next, and its Mono for code" },
  { key: "jetbrains-mono", name: "JetBrains Mono", note: "Monospaced, for the text and the code" },
  { key: "inter", name: "Inter", note: "Used by many apps, like Figma and Linear. Code in JetBrains Mono" },
  { key: "geist", name: "Geist", note: "Vercel's. Code in Geist Mono" },
  { key: "hack", name: "Hack", note: "Warp terminal's default. Monospaced, for the text and the code" },
];

/** The sizes to pick from, in px of each part's main text. The defaults are the sizes Gizai always had. */
export const CHAT_SIZES = [14, 15, 16, 17, 18, 20];
export const UI_SIZES = [12.5, 13.5, 14.5, 15.5, 16.5];
export const DOCS_SIZES = [14, 15, 16, 17, 18, 20];
export const DEFAULTS: Appearance = { font: "atkinson", chat: 15, ui: 13.5, docs: 15, theme: "dark", density: "comfortable" };

/** Where each is kept. gizai-theme and gizai-density are the t and d keys' own, from before. */
export const KEYS: Record<keyof Appearance, string> = {
  font: "gizai-font", chat: "gizai-size-chat", ui: "gizai-size-ui", docs: "gizai-size-docs", theme: "gizai-theme", density: "gizai-density",
};

/** What was kept, with anything unknown (or missing) at its default. */
export function parseAppearance(get: (key: string) => string | null): Appearance {
  const size = (k: "chat" | "ui" | "docs", sizes: number[]) => {
    const n = Number(get(KEYS[k]));
    return sizes.includes(n) ? n : DEFAULTS[k];
  };
  const font = get(KEYS.font);
  return {
    font: FONTS.find((f) => f.key === font)?.key ?? DEFAULTS.font,
    chat: size("chat", CHAT_SIZES), ui: size("ui", UI_SIZES), docs: size("docs", DOCS_SIZES),
    theme: get(KEYS.theme) === "light" ? "light" : "dark",
    density: get(KEYS.density) === "compact" ? "compact" : "comfortable",
  };
}

export function isDefault(a: Appearance): boolean {
  return (Object.keys(DEFAULTS) as (keyof Appearance)[]).every((k) => a[k] === DEFAULTS[k]);
}

/** How much text grows at `size` against its default: reading text fully, headings about half, and small things
 *  (IDs, labels, times, badges) not at all until the largest sizes, and then at most 1px. */
export function growth(size: number, base: number): { text: number; head: number; meta: number } {
  const d = size - base;
  return { text: d, head: d / 2, meta: d >= 3 ? 1 : d >= 2 ? 0.5 : 0 };
}

/** The variables appearance.css works with, in the order they are set. */
export const SIZE_VARS = [
  "--ui-grow", "--ui-grow-head", "--ui-grow-meta", "--ui-box", "--chat-grow", "--chat-grow-head", "--chat-scale", "--docs-grow", "--docs-grow-head",
] as const;

/** The size variables for <html>: how much each part's text grows (px), how much rows and controls grow (--ui-box,
 *  shorter when compact), and how much wider the chat column gets (--chat-scale). Only the ones that aren't zero (or 1):
 *  at the default sizes and density there are none, and the app looks exactly as it always did. */
export function sizeVars(a: Appearance): Partial<Record<(typeof SIZE_VARS)[number], string>> {
  const v: Partial<Record<(typeof SIZE_VARS)[number], string>> = {};
  const px = (name: (typeof SIZE_VARS)[number], n: number) => { if (n !== 0) v[name] = `${n}px`; };
  const ui = growth(a.ui, DEFAULTS.ui);
  px("--ui-grow", ui.text);
  px("--ui-grow-head", ui.head);
  px("--ui-grow-meta", ui.meta);
  px("--ui-box", Math.round(ui.text * 1.5) - (a.density === "compact" ? 4 : 0));
  const chat = growth(a.chat, DEFAULTS.chat);
  px("--chat-grow", chat.text);
  px("--chat-grow-head", chat.head);
  if (a.chat !== DEFAULTS.chat) v["--chat-scale"] = String(Math.round((a.chat / DEFAULTS.chat) * 1000) / 1000);
  const docs = growth(a.docs, DEFAULTS.docs);
  px("--docs-grow", docs.text);
  px("--docs-grow-head", docs.head);
  return v;
}

/** Puts `a` on <html>: the font, theme and density, and the size variables (removed again at their defaults). */
export function applyAppearance(root: HTMLElement, a: Appearance) {
  if (a.font === DEFAULTS.font) delete root.dataset.font;
  else root.dataset.font = a.font;
  root.dataset.theme = a.theme;
  if (a.density === "compact") root.dataset.density = "compact";
  else delete root.dataset.density;
  const vars = sizeVars(a);
  for (const name of SIZE_VARS) {
    const value = vars[name];
    if (value) root.style.setProperty(name, value);
    else root.style.removeProperty(name);
  }
}

function prefs(): Storage | null {
  try { return window.localStorage; } catch { return null; }
}

let current: Appearance | null = null;
const listeners = new Set<(a: Appearance) => void>();

/** What is set now (read from localStorage the first time). */
export function getAppearance(): Appearance {
  if (!current) {
    const s = prefs();
    current = parseAppearance((k) => { try { return s?.getItem(k) ?? null; } catch { return null; } });
  }
  return current;
}

/** Changes part of it: it shows at once and is kept for the next start (a default is kept as no value). */
export function setAppearance(patch: Partial<Appearance>) {
  const next = { ...getAppearance(), ...patch };
  current = next;
  if (typeof document !== "undefined") applyAppearance(document.documentElement, next);
  const s = prefs();
  for (const k of Object.keys(KEYS) as (keyof Appearance)[]) {
    try {
      if (next[k] === DEFAULTS[k]) s?.removeItem(KEYS[k]);
      else s?.setItem(KEYS[k], String(next[k]));
    } catch { /* private mode: it lasts until a restart */ }
  }
  listeners.forEach((f) => f(next));
}

export function resetAppearance() {
  setAppearance(DEFAULTS);
}

/** The t and d keys. */
export function toggleTheme() {
  setAppearance({ theme: getAppearance().theme === "dark" ? "light" : "dark" });
}
export function toggleDensity() {
  setAppearance({ density: getAppearance().density === "compact" ? "comfortable" : "compact" });
}

/** The appearance, kept up to date when it changes (also by the t and d keys). */
export function useAppearance(): Appearance {
  const [a, setA] = useState(getAppearance);
  useEffect(() => {
    listeners.add(setA);
    setA(getAppearance());
    return () => { listeners.delete(setA); };
  }, []);
  return a;
}
