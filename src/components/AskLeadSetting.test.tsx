// GA-70 QA: Settings → Runs → Ask the Team Lead first, the app-wide switch. Rendered to HTML on the server with its state
// handed in (React's useState takes the next value of `queue`); a flip is tried on the element tree with the api mocked.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement, ReactNode } from "react";
import { renderToStaticMarkup } from "react-dom/server";

const queue: unknown[] = [];
const sets: unknown[] = [];
vi.mock("react", async (orig) => {
  const R = await orig<typeof import("react")>();
  const useState = ((init?: unknown) => [queue.length ? queue.shift() : init, (x: unknown) => sets.push(x)]) as unknown as typeof R.useState;
  const useEffect = (() => {}) as typeof R.useEffect;
  return { ...R, default: { ...R, useState, useEffect }, useState, useEffect };
});
const api = vi.hoisted(() => ({ askLeadEnabled: vi.fn(), setAskLeadEnabled: vi.fn() }));
vi.mock("../api", () => api);

const { AskLeadSetting } = await import("./AskLeadSetting");

const said: [boolean, string][] = [];
const say = (ok: boolean, t: string) => said.push([ok, t]);
const render = (on: boolean | null) => { queue.push(on); return renderToStaticMarkup(<AskLeadSetting say={say} />); };

/** The checkbox in the element tree (components called as functions). */
function checkbox(on: boolean | null): { checked: boolean; disabled: boolean; onChange: (e: unknown) => void } {
  queue.push(on);
  const seen: unknown[] = [];
  const walk = (n: ReactNode): void => {
    if (!n || typeof n !== "object") return;
    if (Array.isArray(n)) { n.forEach(walk); return; }
    const el = n as ReactElement<Record<string, unknown>>;
    if (typeof el.type === "function") { walk((el.type as (p: unknown) => ReactNode)(el.props)); return; }
    if (el.props?.type === "checkbox") seen.push(el.props);
    walk(el.props?.children as ReactNode);
    walk(el.props?.hint as ReactNode);
  };
  walk(AskLeadSetting({ say }));
  expect(seen).toHaveLength(1);
  return seen[0] as { checked: boolean; disabled: boolean; onChange: (e: unknown) => void };
}

beforeEach(() => { queue.length = 0; sets.length = 0; said.length = 0; api.setAskLeadEnabled.mockReset(); });

describe("Settings → Runs → Ask the Team Lead first (GA-70)", () => {
  it("is on, and says what always comes to you", () => {
    const html = render(true);
    expect(html).toContain('<input type="checkbox" checked=""/>Ask the Team Lead first');
    expect(html).toContain("An agent that needs a decision asks the Team Lead first");
    expect(html).toContain("Money, scope, deadlines, client messages, security and deleting always come to you.");
  });
  it("says what off means when it is off", () => {
    const html = render(false);
    expect(html).toContain('<input type="checkbox"/>Ask the Team Lead first');
    expect(html).toContain("Off: an agent that needs a decision puts its card in your Inbox at once.");
  });
  it("shows on while it loads, but can't be switched yet", () => {
    expect(checkbox(null)).toMatchObject({ checked: true, disabled: true });
    expect(checkbox(true).disabled).toBe(false);
  });
  it("saves a flip at once, and flips back with the error when saving fails", async () => {
    api.setAskLeadEnabled.mockResolvedValue(undefined);
    checkbox(true).onChange({ target: { checked: false } });
    expect(api.setAskLeadEnabled).toHaveBeenCalledWith(false);
    expect(sets).toEqual([false]);
    await Promise.resolve();
    expect(said).toEqual([]);

    sets.length = 0;
    api.setAskLeadEnabled.mockRejectedValue("database is locked");
    checkbox(false).onChange({ target: { checked: true } });
    expect(api.setAskLeadEnabled).toHaveBeenLastCalledWith(true);
    await new Promise((r) => setTimeout(r, 0));
    expect(sets).toEqual([true, false]);
    expect(said).toEqual([[false, "database is locked"]]);
  });
});
