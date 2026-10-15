// GA-164 QA: how Gizai starts. main.tsx asks onboarding_pending before it draws anything, so the board doesn't show for a
// moment before the onboarding; it hands the answer to App, and when the question fails the app opens as it always did.
// React DOM, App, the api and the appearance are fakes; each test loads main.tsx again.
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";

const h = vi.hoisted(() => ({
  pending: null as null | Promise<boolean>,
  rendered: [] as ReactElement[],
  log: [] as string[],
}));
vi.mock("./api", () => ({ onboardingPending: () => { h.log.push("onboardingPending"); return h.pending!; } }));
vi.mock("react-dom/client", () => ({ default: { createRoot: (el: unknown) => ({ render: (tree: ReactElement) => { h.log.push(`render into ${String((el as { id: string }).id)}`); h.rendered.push(tree); } }) } }));
vi.mock("./App", () => ({ default: function App() { return null; } }));
vi.mock("./lib/appearance", () => ({ getAppearance: () => ({}), applyAppearance: () => { h.log.push("appearance"); } }));
vi.stubGlobal("document", { documentElement: {}, getElementById: (id: string) => ({ id }) });

beforeEach(() => {
  vi.resetModules();
  h.rendered = [];
  h.log = [];
});

/** App's props in what main.tsx rendered (inside React.StrictMode). */
const appProps = () => {
  const strict = h.rendered[0] as ReactElement<{ children: ReactElement<Record<string, unknown>> }>;
  return { name: (strict.props.children.type as { name: string }).name, props: strict.props.children.props };
};
const settle = () => new Promise((r) => setTimeout(r, 0));

describe("main.tsx", () => {
  it("draws nothing until it knows whether the onboarding waits, then App with onboarding true", async () => {
    let answer: (v: boolean) => void = () => {};
    h.pending = new Promise((r) => { answer = r; });
    await import("./main");
    await settle();
    expect(h.log).toEqual(["appearance", "onboardingPending"]);
    expect(h.rendered).toEqual([]);
    answer(true);
    await settle();
    expect(h.log).toEqual(["appearance", "onboardingPending", "render into root"]);
    expect(appProps()).toEqual({ name: "App", props: { onboarding: true } });
  });

  it("an install where it doesn't wait opens the app (onboarding false)", async () => {
    h.pending = Promise.resolve(false);
    await import("./main");
    await settle();
    expect(appProps()).toEqual({ name: "App", props: { onboarding: false } });
  });

  it("when onboarding_pending fails, the app opens as it always did", async () => {
    h.pending = Promise.reject(new Error("no such command"));
    await import("./main");
    await settle();
    expect(appProps()).toEqual({ name: "App", props: { onboarding: false } });
  });
});
