// GA-57: files picked in the New task drawer are kept as paths until Create task; the list shows each one's name and folder,
// and a file picked twice is listed once.
import { describe, expect, it } from "vitest";
import { addPaths, fileExt, fileFolder, fileName } from "./files";

describe("fileName and fileFolder", () => {
  it("split a path into the file's name and the folder it is in", () => {
    expect(fileName("/home/jef/Downloads/invoice 2026.pdf")).toBe("invoice 2026.pdf");
    expect(fileFolder("/home/jef/Downloads/invoice 2026.pdf")).toBe("/home/jef/Downloads");
  });
  it("handle a file at the root, a bare name and a trailing slash", () => {
    expect(fileName("/notes.md")).toBe("notes.md");
    expect(fileFolder("/notes.md")).toBe("/");
    expect(fileName("notes.md")).toBe("notes.md");
    expect(fileFolder("notes.md")).toBe("");
    expect(fileName("/home/jef/shots/")).toBe("shots");
    expect(fileFolder("/home/jef/shots/")).toBe("/home/jef");
  });
});

describe("addPaths", () => {
  it("adds new paths after the ones already listed, in the order picked", () => {
    expect(addPaths(["/a/one.png"], ["/b/two.pdf", "/c/three.txt"])).toEqual(["/a/one.png", "/b/two.pdf", "/c/three.txt"]);
  });
  it("lists a file picked twice once, also when it comes twice in one pick", () => {
    expect(addPaths(["/a/one.png"], ["/a/one.png", "/b/two.pdf", "/b/two.pdf"])).toEqual(["/a/one.png", "/b/two.pdf"]);
  });
  it("keeps two files with the same name from different folders", () => {
    expect(addPaths(["/a/shot.png"], ["/b/shot.png"])).toEqual(["/a/shot.png", "/b/shot.png"]);
  });
  it("leaves the list it is given alone", () => {
    const list = ["/a/one.png"];
    addPaths(list, ["/b/two.pdf"]);
    expect(list).toEqual(["/a/one.png"]);
  });
});

// GA-41: the type badge on a file chip (the chat's files and the New task drawer's).
describe("fileExt", () => {
  it("is the extension, at most 4 letters, upper case", () => {
    expect(fileExt("invoice 2026.pdf")).toBe("PDF");
    expect(fileExt("notes.markdown")).toBe("MARK");
    expect(fileExt("archive.tar.gz")).toBe("GZ");
  });
  it("is FILE without an extension", () => {
    expect(fileExt("Makefile")).toBe("FILE");
  });
});
