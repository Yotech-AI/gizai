// What the tour shows: the captured scenes (capture/<theme>/scenes.json) in the website's order, each with its
// feature's name from the homepage's feature list (design/website/README.md, GA-144) as its caption.
import darkCapture from "../capture/dark/scenes.json";
import lightCapture from "../capture/light/scenes.json";

export type Theme = "dark" | "light";
export type Box = { x: number; y: number; width: number; height: number };
export type Step = { name: string; before: string; after: string; box: Box };
export type CapturedScene = { scene: number; name: string; title: string; steps: Step[] };
export type Caption = { number: string; name: string; line: string };
export type Scene = CapturedScene & { caption: Caption };

/** The stills' size, and the video's: a 13-inch MacBook Air's screen at scale 2, without the title bar. */
export const WIDTH = 2560;
export const HEIGHT = 1664;

/** The website's order, its feature rows 01 to 08: scenes.json lists the scenes in it too. */
export const ORDER = [
  "01-board",
  "02-chat",
  "03-clis",
  "04-review",
  "05-editor",
  "06-memory",
  "07-tools",
  "08-inbox",
];

const CAPTIONS: Record<string, Caption> = {
  "01-board": { number: "01", name: "The board decides who works", line: "Agents pick up the cards on their columns. Watch one at work, live." },
  "02-chat": { number: "02", name: "A Team Lead you talk to", line: "Your request becomes cards. You approve in one click." },
  "03-clis": { number: "03", name: "Bring your own CLI", line: "Claude Code and Codex, with your own logins and their limits." },
  "04-review": { number: "04", name: "Tested before you see it", line: "QA tests each card and opens its pull request." },
  "05-editor": { number: "05", name: "The Editor and Terminal, built in", line: "Open a project, then its terminal, right under the code." },
  "06-memory": { number: "06", name: "Memory", line: "Decisions and gotchas as linked notes, with a graph." },
  "07-tools": { number: "07", name: "Tools per agent", line: "MCP servers, web search and a browser, each off until you switch it on." },
  "08-inbox": { number: "08", name: "The Inbox", line: "Questions, approvals and Run this for me, in one place." },
};

const CAPTURES: Record<Theme, { scenes: CapturedScene[] }> = { dark: darkCapture, light: lightCapture };

/** The theme's scenes in the website's order. Fails loudly when the capture and ORDER don't match. */
export function scenesFor(theme: Theme): Scene[] {
  const captured = CAPTURES[theme].scenes;
  if (captured.length !== ORDER.length) throw new Error(`capture/${theme}/scenes.json has ${captured.length} scenes, the tour ${ORDER.length}`);
  return ORDER.map((name) => {
    const scene = captured.find((s) => s.name === name);
    if (!scene) throw new Error(`capture/${theme}/scenes.json has no scene ${name}`);
    return {
      ...scene,
      steps: scene.steps.map((step) => ({ ...step, before: `${theme}/${step.before}`, after: `${theme}/${step.after}` })),
      caption: CAPTIONS[name],
    };
  });
}

/** The app's tokens (design/tokens.json) and the website hero's mountains at strength strong (design/branding). */
export const COLORS: Record<Theme, {
  bg: string; raised: string; line: string; text: string; text2: string; pink: string;
  cursor: string; cursorShadow: string; shadow: string; bandLine: string; bandGlow: string;
}> = {
  dark: {
    bg: "#0f1013", raised: "#15171b", line: "#30333a", text: "#ededf0", text2: "#a5a8b1", pink: "#f472b6",
    cursor: "#ffffff", cursorShadow: "rgba(0, 0, 0, 0.55)", shadow: "0 24px 64px rgba(0, 0, 0, 0.45)",
    bandLine: "rgba(160, 211, 235, 0.38)", bandGlow: "rgba(20, 100, 128, 0.36)",
  },
  light: {
    bg: "#ffffff", raised: "#ffffff", line: "#d0d3da", text: "#14161b", text2: "#4b4f59", pink: "#c0287a",
    cursor: "#14161b", cursorShadow: "rgba(15, 17, 23, 0.3)", shadow: "0 24px 64px rgba(15, 17, 23, 0.14)",
    bandLine: "rgba(32, 108, 137, 0.50)", bandGlow: "rgba(201, 227, 239, 1)",
  },
};
