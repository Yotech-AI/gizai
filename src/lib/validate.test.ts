import { describe, expect, it } from "vitest";
import { isIban, isKvk, isVatNumber } from "./validate";

describe("NL identifiers", () => {
  it("vat", () => {
    expect(isVatNumber("NL004512378B01")).toBe(true);
    expect(isVatNumber("NL 0045.12378.B01")).toBe(true);
    expect(isVatNumber("DE298765432")).toBe(true);
    expect(isVatNumber("hello")).toBe(false);
    expect(isVatNumber("NL004512378X01")).toBe(false);
  });
  it("kvk", () => {
    expect(isKvk("34098812")).toBe(true);
    expect(isKvk("3409881")).toBe(false);
    expect(isKvk("3409 8812")).toBe(true);
  });
  it("iban", () => {
    expect(isIban("NL91 ABNA 0417 1643 00")).toBe(true);
    expect(isIban("NL91 ABNA 0417 1643 01")).toBe(false);
    expect(isIban("")).toBe(false);
  });
});
