// GA-185 QA: the screenshot settings' defaults, ranges and refusals (lib/screenshotSettings.ts) for the project form →
// Preview, Settings → Agents and runs → Screenshots, the agent's Take screenshots of what it built and the card's
// Screenshots before Review. Each refusal says why with the backend's own words (check_limits, check_project and
// check_start_secs in crates/gizai-core/src/screenshots.rs, read here as text), so the form and the backend agree.
import { describe, expect, it } from "vitest";
import coreShotsRs from "../../crates/gizai-core/src/screenshots.rs?raw";
import mcpServersRs from "../../crates/gizai-core/src/mcp_servers.rs?raw";
import {
  cardShotsHint, checkLimits, checkProjectShots, defaultProjectShots, HEIGHT, KNOWN_SIZES, LIMITS, limitsProblem, nextSize, portBlocks,
  PORTS_PER_RUN, projectShotsFrom, projectShotsProblem, screenshotsDefault, shownNumber, SIZE_NAME_CHARS, SIZES, sizeName, START_SECS, typedNumber,
  WIDTH, type LimitKey,
} from "./screenshotSettings";

/** The format strings of the backend's messages, as regexes: each `{…}` stands for any value. */
const rustMessages = [...coreShotsRs.matchAll(/format!\("((?:[^"\\]|\\[\s\S])*)"/g)]
  .map((m) => m[1].replace(/\\\n\s*/g, ""))
  .filter((f) => /must|need/.test(f))
  .map((f) => new RegExp(`^${f.split(/\{[^}]*\}/).map((p) => p.replace(/[.*+?^$()|[\]\\]/g, "\\$&")).join(".+?")}$`));
/** True when the message is one the backend writes too. */
const backendSays = (message: string | null | undefined) => !!message && rustMessages.some((r) => r.test(message));

const defaults = { screenshotsPerRun: 12, screenshotMaxMb: 10, screenshotsKeepSets: 3, screenshotsKeepDays: 30, previewPortFrom: 47100, previewPortTo: 47999, maxConcurrentRuns: 3 };

describe("Settings → Agents and runs → Screenshots", () => {
  it("takes the defaults, and 0 days (0 keeps them)", () => {
    expect(limitsProblem(defaults)).toBeNull();
    expect(checkLimits({ ...defaults, screenshotsKeepDays: 0 })).toBeNull();
    // each at its lowest and highest (the ports as a pair, the lower one first)
    for (const key of (Object.keys(LIMITS) as LimitKey[]).filter((k) => !k.startsWith("previewPort"))) {
      expect(checkLimits({ ...defaults, [key]: LIMITS[key].low }), `${key} low`).toBeNull();
      expect(checkLimits({ ...defaults, [key]: LIMITS[key].high }), `${key} high`).toBeNull();
    }
    expect(checkLimits({ ...defaults, previewPortFrom: 1024, previewPortTo: 65535 })).toBeNull();
  });

  it("refuses each value below or above its range, naming the field, the range and its unit, as GA-181's notes write it", () => {
    expect(checkLimits({ ...defaults, screenshotsPerRun: 31 })).toBe("Screenshots per run must be from 1 to 30.");
    expect(checkLimits({ ...defaults, screenshotsPerRun: 0 })).toBe("Screenshots per run must be from 1 to 30.");
    expect(checkLimits({ ...defaults, screenshotMaxMb: 51 })).toBe("Largest screenshot must be from 1 to 50 MB.");
    expect(checkLimits({ ...defaults, screenshotsKeepSets: 21 })).toBe("Keep per card must be from 1 to 20 sets.");
    expect(checkLimits({ ...defaults, screenshotsKeepDays: 366 })).toBe("Delete a finished card’s screenshots after must be from 0 to 365 days.");
    expect(checkLimits({ ...defaults, screenshotsKeepDays: -1 })).toBe("Delete a finished card’s screenshots after must be from 0 to 365 days.");
    expect(checkLimits({ ...defaults, previewPortFrom: 1023 })).toBe("Preview ports must be from 1024 to 65535.");
    expect(checkLimits({ ...defaults, previewPortTo: 65536 })).toBe("Preview ports must be from 1024 to 65535.");
  });

  it("names the value it refuses, so its field can say why and take the focus", () => {
    expect(limitsProblem({ ...defaults, screenshotMaxMb: 0 })?.key).toBe("screenshotMaxMb");
    expect(limitsProblem({ ...defaults, previewPortTo: 70000 })?.key).toBe("previewPortTo");
    // the first from the top when two are wrong
    expect(limitsProblem({ ...defaults, screenshotsKeepSets: 0, screenshotsPerRun: 99 })?.key).toBe("screenshotsPerRun");
  });

  it("refuses an emptied field (NaN while typing) and a fraction", () => {
    expect(limitsProblem({ ...defaults, screenshotsPerRun: typedNumber("") })?.key).toBe("screenshotsPerRun");
    expect(limitsProblem({ ...defaults, screenshotMaxMb: 2.5 })?.key).toBe("screenshotMaxMb");
  });

  it("wants the preview ports lowest first, with ten for each run at once", () => {
    expect(checkLimits({ ...defaults, previewPortFrom: 48000, previewPortTo: 47999 }))
      .toBe("Preview ports must go from the lower port to the higher one, not from 48000 to 47999.");
    expect(limitsProblem({ ...defaults, previewPortFrom: 48000, previewPortTo: 47999 })?.key).toBe("previewPortFrom");
    expect(checkLimits({ ...defaults, previewPortFrom: 47100, previewPortTo: 47110 }))
      .toBe("Preview ports must hold at least ten ports for each run at once: 47100 to 47110 has room for 1 run, and Runs at once (Settings → Runs) is 3.");
    expect(checkLimits({ ...defaults, previewPortFrom: 47100, previewPortTo: 47100 }))
      .toBe("Preview ports must hold at least ten ports for each run at once: 47100 to 47100 has room for 0 runs, and Runs at once (Settings → Runs) is 3.");
    expect(limitsProblem({ ...defaults, previewPortTo: 47110 })?.key).toBe("previewPortTo");
    // 30 ports hold three runs exactly
    expect(checkLimits({ ...defaults, previewPortFrom: 47100, previewPortTo: 47129 })).toBeNull();
    // without Runs at once, or with one outside its range (the backend's to refuse), the room isn't checked
    expect(checkLimits({ ...defaults, previewPortTo: 47110, maxConcurrentRuns: undefined })).toBeNull();
    expect(checkLimits({ ...defaults, previewPortTo: 47110, maxConcurrentRuns: 99 })).toBeNull();
    expect([portBlocks(47100, 47999), portBlocks(47100, 47109), portBlocks(47100, 47108), portBlocks(2, 1), PORTS_PER_RUN]).toEqual([90, 1, 0, 0, 10]);
  });

  it("leaves out what the backend didn't send", () => {
    expect(limitsProblem({})).toBeNull();
    expect(limitsProblem({ screenshotsPerRun: 12 })).toBeNull();
  });

  it("says why in the backend's words", () => {
    expect(rustMessages.length).toBeGreaterThanOrEqual(7);
    expect(backendSays("Screenshots per run has to be 1 to 30.")).toBe(false);
    for (const s of [{ screenshotsPerRun: 0 }, { screenshotMaxMb: 99 }, { screenshotsKeepSets: 0 }, { screenshotsKeepDays: 400 }, { previewPortFrom: 80 },
      { previewPortFrom: 48000, previewPortTo: 47999 }, { previewPortTo: 47105 }]) {
      const message = checkLimits({ ...defaults, ...s });
      expect(backendSays(message), String(message)).toBe(true);
    }
  });
});

describe("the project form → Preview", () => {
  const ok = () => ({ screenshots: defaultProjectShots(), previewStartSecs: 120 });

  it("starts with Screenshots on, Desktop 1280 × 800 and Mobile 390 × 844, one theme, one window's height and 120 seconds, as the backend's default", () => {
    expect(defaultProjectShots()).toEqual({ on: true, sizes: [{ name: "Desktop", width: 1280, height: 800 }, { name: "Mobile", width: 390, height: 844 }],
      bothThemes: false, fullPage: false });
    expect(coreShotsRs).toMatch(/ProjectScreenshots \{ on: true, sizes: default_sizes\(\), both_themes: false, full_page: false \}/);
    expect(coreShotsRs).toContain(`ShotSize { name: "Desktop".into(), width: 1280, height: 800 }, ShotSize { name: "Mobile".into(), width: 390, height: 844 }`);
    expect(START_SECS).toEqual({ low: 10, high: 600, default: 120 });
    expect(coreShotsRs).toContain("pub const START_SECS: RangeInclusive<u32> = 10..=600;");
    expect([SIZES, WIDTH, HEIGHT, SIZE_NAME_CHARS]).toEqual([{ low: 1, high: 4 }, { low: 320, high: 3840 }, { low: 320, high: 2400 }, 40]);
    expect(coreShotsRs).toContain("pub const SIZES: RangeInclusive<usize> = 1..=4;");
    expect(coreShotsRs).toContain("pub const WIDTH: RangeInclusive<u32> = 320..=3840;");
    expect(coreShotsRs).toContain("pub const HEIGHT: RangeInclusive<u32> = 320..=2400;");
    expect(coreShotsRs).toContain("const SIZE_NAME_CHARS: usize = 40;");
    expect(projectShotsProblem(ok())).toBeNull();
  });

  it("refuses a Start time outside 10 to 600 seconds, and an emptied one", () => {
    for (const secs of [9, 601, 0, typedNumber(" "), 12.5]) {
      expect(projectShotsProblem({ ...ok(), previewStartSecs: secs }), String(secs)).toEqual({ at: "start", message: "Start time must be from 10 to 600 seconds." });
    }
    for (const secs of [10, 600]) expect(projectShotsProblem({ ...ok(), previewStartSecs: secs })).toBeNull();
    // left out: the backend keeps the saved one
    expect(projectShotsProblem({ screenshots: defaultProjectShots(), previewStartSecs: null })).toBeNull();
  });

  it("wants one to four sizes", () => {
    expect(checkProjectShots({ ...ok(), screenshots: { ...defaultProjectShots(), sizes: [] } })).toBe("Screenshots need 1 to 4 sizes, not 0.");
    expect(checkProjectShots({ ...ok(), screenshots: { ...defaultProjectShots(), sizes: [...KNOWN_SIZES, { name: "Tablet", width: 768, height: 1024 }] } }))
      .toBe("Screenshots need 1 to 4 sizes, not 5.");
    expect(checkProjectShots({ ...ok(), screenshots: { ...defaultProjectShots(), sizes: [...KNOWN_SIZES] } })).toBeNull();
    expect(checkProjectShots({ ...ok(), screenshots: { ...defaultProjectShots(), sizes: [KNOWN_SIZES[2]] } })).toBeNull();
  });

  it("wants each size named, at most 40 characters, and 320 to 3,840 wide and 320 to 2,400 high, saying which", () => {
    const sizes = (s: { name: string; width: number; height: number }) => ({ ...ok(), screenshots: { ...defaultProjectShots(), sizes: [KNOWN_SIZES[0], s] } });
    const name = "Each size needs a name of at most 40 characters, like Desktop.";
    expect(projectShotsProblem(sizes({ name: "", width: 800, height: 600 }))).toEqual({ at: "name", size: 1, message: name });
    expect(projectShotsProblem(sizes({ name: "   ", width: 800, height: 600 }))).toEqual({ at: "name", size: 1, message: name });
    expect(projectShotsProblem(sizes({ name: "x".repeat(41), width: 800, height: 600 }))?.at).toBe("name");
    expect(projectShotsProblem(sizes({ name: "Tab\u0007let", width: 800, height: 600 }))?.at).toBe("name");
    expect(projectShotsProblem(sizes({ name: "x".repeat(40), width: 800, height: 600 }))).toBeNull();
    // the name as the backend keeps it: trimmed, one space between words
    expect(projectShotsProblem(sizes({ name: "  Big   screen ", width: 319, height: 600 })))
      .toEqual({ at: "width", size: 1, message: "Big screen must be from 320 to 3,840 pixels wide, not 319." });
    expect(checkProjectShots(sizes({ name: "Wide", width: 3841, height: 600 }))).toBe("Wide must be from 320 to 3,840 pixels wide, not 3841.");
    expect(projectShotsProblem(sizes({ name: "Tall", width: 800, height: 2401 })))
      .toEqual({ at: "height", size: 1, message: "Tall must be from 320 to 2,400 pixels high, not 2401." });
    expect(checkProjectShots(sizes({ name: "Low", width: 800, height: 100 }))).toBe("Low must be from 320 to 2,400 pixels high, not 100.");
    // an emptied field says the range without "not NaN"
    expect(checkProjectShots(sizes({ name: "Tablet", width: typedNumber(""), height: 600 }))).toBe("Tablet must be from 320 to 3,840 pixels wide.");
    expect(checkProjectShots(sizes({ name: "Edge", width: 320, height: 2400 }))).toBeNull();
    expect(sizeName("  Big   screen ")).toBe("Big screen");
  });

  it("checks the Start time first, then the sizes from the top", () => {
    const bad = { screenshots: { ...defaultProjectShots(), sizes: [{ name: "", width: 1, height: 1 }, { name: "B", width: 1, height: 1 }] }, previewStartSecs: 5 };
    expect(projectShotsProblem(bad)?.at).toBe("start");
    expect(projectShotsProblem({ ...bad, previewStartSecs: 120 })).toEqual({ at: "name", size: 0, message: "Each size needs a name of at most 40 characters, like Desktop." });
  });

  it("says why in the backend's words", () => {
    for (const v of [{ previewStartSecs: 5 }, { screenshots: { ...defaultProjectShots(), sizes: [] } },
      { screenshots: { ...defaultProjectShots(), sizes: [{ name: "", width: 800, height: 600 }] } },
      { screenshots: { ...defaultProjectShots(), sizes: [{ name: "Desktop", width: 100, height: 800 }] } },
      { screenshots: { ...defaultProjectShots(), sizes: [{ name: "Desktop", width: 1280, height: 9000 }] } }]) {
      const message = checkProjectShots({ screenshots: defaultProjectShots(), previewStartSecs: 120, ...v });
      expect(backendSays(message), String(message)).toBe(true);
    }
  });

  it("Add a size adds the first known size the project doesn't have yet: Window, then Small", () => {
    const two = defaultProjectShots().sizes;
    expect(nextSize(two)).toEqual({ name: "Window", width: 1440, height: 900 });
    expect(nextSize([...two, KNOWN_SIZES[2]])).toEqual({ name: "Small", width: 900, height: 600 });
    expect(nextSize([{ name: " desktop ", width: 1, height: 1 }])).toEqual({ name: "Mobile", width: 390, height: 844 });
    expect(nextSize([...KNOWN_SIZES])).toEqual({ name: "Desktop", width: 1280, height: 800 });
    // a copy: editing the new size leaves the known ones alone
    const s = nextSize(two);
    s.width = 1;
    expect(KNOWN_SIZES[2].width).toBe(1440);
  });

  it("the form starts from the project's own values, with the defaults for what it doesn't have", () => {
    expect(projectShotsFrom(null)).toEqual(defaultProjectShots());
    expect(projectShotsFrom({ on: false, sizes: [] })).toEqual({ ...defaultProjectShots(), on: false });
    const own = { on: true, sizes: [{ name: "Window", width: 1440, height: 900 }], bothThemes: true, fullPage: true };
    const v = projectShotsFrom(own);
    expect(v).toEqual(own);
    v.sizes[0].width = 1;
    expect(own.sizes[0].width).toBe(1440);
  });

  it("a number field shows nothing for an emptied value, and keeps what is typed", () => {
    expect([shownNumber(null), shownNumber(undefined), shownNumber(Number.NaN), shownNumber(0), shownNumber(120)]).toEqual(["", "", "", 0, 120]);
    expect([typedNumber("120"), typedNumber(" 7 ")]).toEqual([120, 7]);
    expect(Number.isNaN(typedNumber(""))).toBe(true);
  });
});

describe("the agent's Take screenshots of what it built", () => {
  it("is on by default for the Frontend, Design and QA agents and off for the rest, as the backend's screenshots_default", () => {
    expect(["frontend", "design", "qa"].map(screenshotsDefault)).toEqual([true, true, true]);
    expect(["backend", "devops", "lead", "agent", "", "Frontend"].map(screenshotsDefault)).toEqual([false, false, false, false, false, false]);
    expect(mcpServersRs).toMatch(/pub fn screenshots_default\(role: &str\) -> bool \{\s*matches!\(role, "frontend" \| "design" \| "qa"\)/);
  });
});

describe("the card's Screenshots before Review", () => {
  it("says whether it is the card's own or follows the project's switch, in GA-181's words", () => {
    expect(cardShotsHint(false, true, "gizai-website")).toBe("Off for this card. By default it follows gizai-website’s Screenshots switch, which is on.");
    expect(cardShotsHint(true, false, "gizai-website")).toBe("On for this card. By default it follows gizai-website’s Screenshots switch, which is off.");
    expect(cardShotsHint(null, true, "gizai-website")).toBe("Follows gizai-website’s Screenshots switch, which is on.");
    expect(cardShotsHint(undefined, false, "Kade portal")).toBe("Follows Kade portal’s Screenshots switch, which is off.");
    // while the project loads
    expect(cardShotsHint(null, undefined, "Kade portal")).toBe("Follows Kade portal’s Screenshots switch.");
  });
});
