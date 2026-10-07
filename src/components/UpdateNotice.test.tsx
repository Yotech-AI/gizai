// GA-1: the update notice above Company in the sidebar, and Settings → Updates. Rendered to HTML on the server with
// the update status given (useUpdate is replaced), so no data loads and nothing calls Gizai.
import { beforeEach, describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { Release, UpdateJob, UpdateStatus, UpdateStep } from "../types";

let current: UpdateStatus | null = null;
vi.mock("../lib/useUpdate", () => ({ useUpdate: () => [current, () => {}] }));

const { UpdateNotice } = await import("./UpdateNotice");
const { UpdateSettings } = await import("./UpdateSettings");
const { Sidebar } = await import("./Sidebar");

const release: Release = { version: "0.1.6", tag: "v0.1.6", url: "https://github.com/Yotech-AI/gizai/releases/tag/v0.1.6", notes: "- Faster boards" };
const status = (more: Partial<UpdateStatus> = {}): UpdateStatus => ({
  current: "0.1.5", autoCheck: true, checking: false, repo: "https://github.com/Yotech-AI/gizai.git", checkedAt: Date.now(),
  installTo: "/home/you/.local", ...more,
});
const job = (step: UpdateStep, more: Partial<UpdateJob> = {}): UpdateJob =>
  ({ version: "0.1.6", step, startedAt: 1, log: "/home/you/.local/share/gizai/update/update.log", unchanged: true, ...more });
const text = (html: string) => html.replace(/<[^>]+>/g, " ").replace(/&#x27;/g, "'").replace(/&quot;/g, '"').replace(/&amp;/g, "&").replace(/\s+/g, " ");

beforeEach(() => { current = null; });

describe("the notice above Company", () => {
  it("shows Update to <version> above Company when a newer release is out", () => {
    current = status({ latest: release, available: release });
    const html = renderToStaticMarkup(<Sidebar route={{ page: "board" } as never} youId="u1" onSearch={() => {}} onNewTask={() => {}} />);
    const notice = html.indexOf("Update to 0.1.6");
    const company = html.indexOf(">Company<");
    expect(notice).toBeGreaterThan(-1);
    expect(company).toBeGreaterThan(notice);
  });
  it("isn't there when this is the latest version", () => {
    current = status({ latest: { ...release, version: "0.1.5", tag: "v0.1.5" } });
    expect(renderToStaticMarkup(<UpdateNotice />)).toBe("");
    const html = renderToStaticMarkup(<Sidebar route={{ page: "board" } as never} youId="u1" onSearch={() => {}} onNewTask={() => {}} />);
    expect(html).not.toContain("update-notice");
  });
  it("is a button that starts the update, saying what it does", () => {
    current = status({ available: release });
    const html = renderToStaticMarkup(<UpdateNotice />);
    expect(html).toMatch(/<button class="update-notice"[^>]*title="Builds it in the background, backs up your data, installs it and offers a restart"/);
    expect(text(html)).toContain("Update to 0.1.6");
  });
  it("links to Settings on a Gizai that can't update itself", () => {
    current = status({ available: release, installTo: null, cannotInstall: "This Gizai runs from /repo/target/release/gizai, not from an install" });
    const html = renderToStaticMarkup(<UpdateNotice />);
    expect(html).toMatch(/<a class="update-notice" href="#\/settings"/);
    expect(html).not.toContain("<button");
  });
  it("shows the step while it builds, and links to Settings", () => {
    current = status({ available: release, job: job("build") });
    const html = renderToStaticMarkup(<UpdateNotice />);
    expect(html).toContain('class="update-notice working"');
    expect(text(html)).toContain("Building 0.1.6…");
  });
  it("offers the restart once it is installed", () => {
    current = status({ available: release, installed: "0.1.6", job: job("installed", { unchanged: false, backup: "/b.db" }) });
    const html = renderToStaticMarkup(<UpdateNotice />);
    expect(html).toContain('class="update-notice ready"');
    expect(text(html)).toContain("Restart to use 0.1.6");
  });
  it("says the update failed, with why in its title", () => {
    current = status({ available: release, job: job("failed", { problem: "./install.sh --build-only failed (exit code 101)" }) });
    const html = renderToStaticMarkup(<UpdateNotice />);
    expect(html).toContain('class="update-notice failed"');
    expect(html).toContain('title="./install.sh --build-only failed (exit code 101)"');
    expect(text(html)).toContain("Update to 0.1.6 failed");
  });
});

describe("Settings → Updates", () => {
  it("has the automatic check switch (on) and Check now", () => {
    current = status({ latest: { ...release, version: "0.1.5", tag: "v0.1.5" } });
    const html = renderToStaticMarkup(<UpdateSettings />);
    expect(html).toContain("Updates");
    expect(html).toMatch(/<input type="checkbox" checked=""\/>Check for new releases automatically/);
    expect(text(html)).toContain("Check now");
    expect(text(html)).toContain("Gizai 0.1.5. This is the latest version (checked just now).");
    expect(html).not.toContain("btn primary");
  });
  it("shows the switch off, and Checking… while a check runs", () => {
    current = status({ autoCheck: false, checking: true });
    const html = renderToStaticMarkup(<UpdateSettings />);
    expect(html).toMatch(/<input type="checkbox"\/>Check for new releases automatically/);
    expect(html).toMatch(/<button class="btn" disabled="">.*Checking…<\/button>/);
  });
  it("offers the newer release with its notes and its page on GitHub", () => {
    current = status({ latest: release, available: release });
    const t = text(renderToStaticMarkup(<UpdateSettings />));
    expect(t).toContain("Version 0.1.6 is out");
    expect(t).toContain("Update to 0.1.6");
    expect(t).toContain("What's new in 0.1.6");
    expect(t).toContain("Faster boards");
    expect(t).toContain("Release 0.1.6 on GitHub");
    expect(t).toContain("Installs into /home/you/.local.");
  });
  it("links only to a release page on https", () => {
    current = status({ available: { ...release, url: "javascript:alert(1)" } });
    expect(text(renderToStaticMarkup(<UpdateSettings />))).not.toContain("Release 0.1.6 on GitHub");
  });
  it("shows the step and Stop while it builds, and no Stop while it installs", () => {
    current = status({ available: release, job: job("build") });
    let t = text(renderToStaticMarkup(<UpdateSettings />));
    expect(t).toContain("Building 0.1.6…");
    expect(t).toContain("You can keep working.");
    expect(t).toContain("Stop");
    current = status({ available: release, job: job("install", { backup: "/home/you/.local/share/gizai/backups/gizai-before-update-x.db" }) });
    t = text(renderToStaticMarkup(<UpdateSettings />));
    expect(t).toContain("Installing 0.1.6…");
    expect(t).not.toContain("Stop");
    expect(t).toContain("Your data was backed up to /home/you/.local/share/gizai/backups/gizai-before-update-x.db");
  });
  it("says why an update failed, that the installed version still works, with the output, the log and Try again", () => {
    current = status({ available: release, job: job("failed", { problem: "./install.sh --build-only failed (exit code 101)", output: "error[E0425]: cannot find value" }) });
    const html = renderToStaticMarkup(<UpdateSettings />);
    const t = text(html);
    expect(html).toContain('role="alert"');
    expect(t).toContain("The update to 0.1.6 didn't work: ./install.sh --build-only failed (exit code 101). Gizai 0.1.5 is still installed and works as before.");
    expect(t).toContain("error[E0425]: cannot find value");
    expect(t).toContain("Everything it did is in /home/you/.local/share/gizai/update/update.log");
    expect(t).toContain("Try again");
  });
  it("says so when the install stopped partway", () => {
    current = status({ available: release, job: job("failed", { problem: "./install.sh --skip-build was ended by a signal", unchanged: false }) });
    expect(text(renderToStaticMarkup(<UpdateSettings />))).toContain("The installer stopped partway, so the installed Gizai may not start");
  });
  it("offers Restart Gizai once it is installed, with the backup", () => {
    current = status({ available: release, installed: "0.1.6", job: job("installed", { unchanged: false, backup: "/home/you/.local/share/gizai/backups/gizai-before-update-x.db" }) });
    const t = text(renderToStaticMarkup(<UpdateSettings />));
    expect(t).toContain("Version 0.1.6 is installed. Restart Gizai to use it.");
    expect(t).toContain("Restart Gizai");
    expect(t).toContain("gizai-before-update-x.db");
  });
});
