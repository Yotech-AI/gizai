// GA-113 QA: Settings → General → Editor folders (src/lib/editorRoots.ts): the folders a save sends, what each row was
// saved as, the backend's refusal as a sentence under its row, the folders that aren't there now, which folders are
// directly inside an Editor folder (also on Windows), and the line under a saved folder.
import { describe, expect, it } from "vitest";
import type { EditorFolder } from "../types";
import { inRoot, isSaved, keptRows, MISSING, missingOf, refusalOf, rootSummary, rootsToSave, savedAs, sentence } from "./editorRoots";

describe("the Editor folders a save sends (GA-113)", () => {
  it("trims each row and leaves out the empty ones", () => {
    expect(rootsToSave([" ~/Herd ", "", "   ", "/srv/projects/"])).toEqual(["~/Herd", "/srv/projects/"]);
    expect(rootsToSave([])).toEqual([]);
    expect(rootsToSave(["", " "])).toEqual([]);
  });
});

describe("what a row was saved as (GA-113)", () => {
  const sent = ["/home/you/Herd/", "~/Code"];
  const kept = ["~/Herd", "~/Code"];
  it("is what the backend kept of the folder the save sent, in the same place", () => {
    expect(savedAs("/home/you/Herd/", sent, kept)).toBe("~/Herd");
    expect(savedAs("  /home/you/Herd/  ", sent, kept)).toBe("~/Herd");
    expect(savedAs("~/Code", sent, kept)).toBe("~/Code");
  });
  it("is the row itself when it is written as the backend kept it", () => {
    expect(savedAs("~/Herd", [], ["~/Herd"])).toBe("~/Herd");
    expect(savedAs(" ~/Herd ", [], ["~/Herd"])).toBe("~/Herd");
  });
  it("is null for a row that isn't saved yet, and for an empty row", () => {
    expect(savedAs("~/New", sent, kept)).toBeNull();
    expect(savedAs("", sent, kept)).toBeNull();
    expect(savedAs("", [], [""])).toBeNull();
  });
  it("is null for a folder sent that the backend kept nothing for", () => {
    expect(savedAs("/b", ["/a", "/b"], ["~/a"])).toBeNull();
  });
});

describe("whether the Editor folders are saved already (GA-113)", () => {
  it("is so when each is saved, as typed or as the backend kept it, in the same order", () => {
    expect(isSaved(["/home/you/Herd/", "~/Code"], ["/home/you/Herd/", "~/Code"], ["~/Herd", "~/Code"])).toBe(true);
    expect(isSaved(["~/Herd", "~/Code"], ["/home/you/Herd/", "~/Code"], ["~/Herd", "~/Code"])).toBe(true);
    expect(isSaved([], [], [])).toBe(true);
  });
  it("isn't with a folder more or less", () => {
    expect(isSaved(["~/Herd", "~/New"], ["~/Herd"], ["~/Herd"])).toBe(false);
    expect(isSaved([], ["~/Herd"], ["~/Herd"])).toBe(false);
  });
  it("isn't in another order", () => {
    expect(isSaved(["~/Code", "~/Herd"], ["~/Herd", "~/Code"], ["~/Herd", "~/Code"])).toBe(false);
  });
  it("isn't with a folder changed", () => {
    expect(isSaved(["~/Herds"], ["~/Herd"], ["~/Herd"])).toBe(false);
  });
});

describe("the rows after a save (GA-113)", () => {
  const sent = ["/home/you/Herd/", "~/Code"];
  const kept = ["~/Herd", "~/Code"];
  it("writes each saved row as the backend kept it, the others as they are", () => {
    expect(keptRows(["/home/you/Herd/", "~/New", "", "~/Code"], sent, kept, null)).toEqual(["~/Herd", "~/New", "", "~/Code"]);
  });
  it("leaves the row you are typing in as you typed it", () => {
    expect(keptRows(["/home/you/Herd/", "~/Code"], sent, kept, 0)).toEqual(["/home/you/Herd/", "~/Code"]);
    expect(keptRows(["/home/you/Herd/", "~/Code"], sent, kept, 1)).toEqual(["~/Herd", "~/Code"]);
  });
});

describe("a sentence (GA-113)", () => {
  it("starts with a capital and ends with a full stop", () => {
    expect(sentence("that's your home folder")).toBe("That's your home folder.");
    expect(sentence("  it holds keys or logins (~/.ssh)  ")).toBe("It holds keys or logins (~/.ssh).");
  });
  it("keeps the stop it has", () => {
    expect(sentence("done.")).toBe("Done.");
    expect(sentence("really?")).toBe("Really?");
    expect(sentence("now!")).toBe("Now!");
    expect(sentence("wait…")).toBe("Wait…");
  });
  it("is empty for nothing", () => {
    expect(sentence("")).toBe("");
    expect(sentence("   ")).toBe("");
  });
});

describe("the refusal in a save's error (GA-113)", () => {
  it("gives the folder as typed and why, as a sentence", () => {
    expect(refusalOf("Editor folders: ~ can't be used: that's your home folder: pick the folder that holds your projects inside it")).toEqual({
      path: "~", why: "That's your home folder: pick the folder that holds your projects inside it.",
    });
    expect(refusalOf("Editor folders: / can't be used: that's the whole disk (/): pick the folder that holds your projects")).toEqual({
      path: "/", why: "That's the whole disk (/): pick the folder that holds your projects.",
    });
  });
  it("finds it after what comes before it, and keeps a folder with spaces whole", () => {
    expect(refusalOf("Error: invalid: Editor folders: ~/My Projects can't be used: that's a file: pick a folder")).toEqual({
      path: "~/My Projects", why: "That's a file: pick a folder.",
    });
    expect(refusalOf("  Editor folders: Herd can't be used: give the full path, like ~/Herd or /srv/projects  ")).toEqual({
      path: "Herd", why: "Give the full path, like ~/Herd or /srv/projects.",
    });
  });
  it("is null for any other error", () => {
    expect(refusalOf("Couldn't save the settings: disk full")).toBeNull();
    expect(refusalOf("Folders: ~ can't be used: that's your home folder")).toBeNull();
    expect(refusalOf("")).toBeNull();
  });
});

describe("the Editor folders that aren't there now (GA-113)", () => {
  it("are the ones with a warning and no refusal, from the check in the same order", () => {
    const paths = ["~/Herd", "/mnt/usb/projects", "~/.ssh", "~/Code"];
    expect(missingOf(paths, [
      { path: "/home/you/Herd" },
      { path: "/mnt/usb/projects", warning: "It isn't there", error: null },
      { path: "/home/you/.ssh", warning: "It isn't there", error: "It holds keys" },
      { path: "/home/you/Code", warning: null, error: null },
    ])).toEqual(["/mnt/usb/projects"]);
  });
  it("is none without a check for them", () => {
    expect(missingOf(["~/Herd", "~/Code"], [])).toEqual([]);
  });
  it("tells you it is saved all the same", () => {
    expect(MISSING).toBe("It isn't there now: the picker lists its folders once it is.");
  });
});

describe("a folder directly inside an Editor folder (GA-113)", () => {
  it("is in it", () => {
    expect(inRoot("~/Herd", "~/Herd/kade-portal")).toBe(true);
    expect(inRoot("/srv/projects", "/srv/projects/shop")).toBe(true);
  });
  it("is in it with slashes at the end of either", () => {
    expect(inRoot("~/Herd/", "~/Herd/kade-portal")).toBe(true);
    expect(inRoot("~/Herd//", "~/Herd/kade-portal/")).toBe(true);
    expect(inRoot(" ~/Herd ", "~/Herd/kade-portal")).toBe(true);
  });
  it("is in it on Windows, with backslashes", () => {
    expect(inRoot("C:\\Users\\you\\Herd", "C:\\Users\\you\\Herd\\app")).toBe(true);
    expect(inRoot("C:\\Users\\you\\Herd\\", "C:\\Users\\you\\Herd\\app\\")).toBe(true);
    expect(inRoot("~\\Herd", "~\\Herd\\app")).toBe(true);
    expect(inRoot("C:\\Users\\you\\Herd", "C:\\Users\\you\\Herd\\app\\deeper")).toBe(false);
  });
  it("isn't when it is deeper, the folder itself, a name that only starts the same or elsewhere", () => {
    expect(inRoot("~/Herd", "~/Herd/kade-portal/src")).toBe(false);
    expect(inRoot("~/Herd", "~/Herd")).toBe(false);
    expect(inRoot("~/Herd", "~/Herd/")).toBe(false);
    expect(inRoot("~/Herd", "~/Herdsman/x")).toBe(false);
    expect(inRoot("~/Her", "~/Herd/x")).toBe(false);
    expect(inRoot("~/Code", "~/Herd/x")).toBe(false);
    expect(inRoot("~/Herd", "kade-portal")).toBe(false);
  });
  it("isn't for an empty Editor folder", () => {
    expect(inRoot("", "/x")).toBe(false);
    expect(inRoot("  ", "/x")).toBe(false);
  });
});

describe("the line under a saved Editor folder (GA-113)", () => {
  const ef = (kind: EditorFolder["kind"], path: string): EditorFolder => {
    const name = path.slice(path.lastIndexOf("/") + 1);
    return { id: path.replace(/^~/, "/home/you"), path, name, kind, projectId: null, taskId: null, readOnly: false, readOnlyWhy: null };
  };
  it("says there are none yet", () => {
    expect(rootSummary("~/Herd", [])).toBe("No folders in it yet.");
    expect(rootSummary("~/Herd", [ef("root", "~/Code/tool")])).toBe("No folders in it yet.");
  });
  it("counts only the folders the Editor opens directly inside it (kind root)", () => {
    expect(rootSummary("~/Herd", [ef("project", "~/Herd/kade-portal"), ef("card", "~/Herd/GA-1"), ef("root", "~/Herd/a/b")])).toBe("No folders in it yet.");
  });
  it("names one folder", () => {
    expect(rootSummary("~/Herd/", [ef("root", "~/Herd/kade-portal")])).toBe("1 folder: kade-portal.");
  });
  it("names two and three folders", () => {
    expect(rootSummary("~/Herd", [ef("root", "~/Herd/a"), ef("root", "~/Herd/b")])).toBe("2 folders: a and b.");
    expect(rootSummary("~/Herd", [ef("root", "~/Herd/gizai-site"), ef("root", "~/Herd/invoice-api"), ef("root", "~/Herd/kade-portal")]))
      .toBe("3 folders: gizai-site, invoice-api and kade-portal.");
  });
  it("names the first three of more and counts the rest", () => {
    const names = ["gizai-site", "invoice-api", "kade-portal", ...Array.from({ length: 11 }, (_, i) => `p${i}`)];
    expect(rootSummary("~/Herd", names.map((n) => ef("root", `~/Herd/${n}`)))).toBe("14 folders: gizai-site, invoice-api, kade-portal and 11 more.");
    expect(rootSummary("~/Herd", ["a", "b", "c", "d"].map((n) => ef("root", `~/Herd/${n}`)))).toBe("4 folders: a, b, c and 1 more.");
  });
});
