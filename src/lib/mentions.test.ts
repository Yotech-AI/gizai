import { describe, expect, it } from "vitest";
import { findMentions, findTaskRefs } from "./mentions";
describe("refs", () => {
  it("finds task refs and mentions", () => {
    const md = "See KADE-41 and GFW-7; ask @jeffrey and @qa-agent. Not a-1 or email x@y.nl";
    expect(findTaskRefs(md)).toEqual(["KADE-41", "GFW-7"]);
    expect(findMentions(md)).toEqual(["jeffrey", "qa-agent"]);
  });
});
