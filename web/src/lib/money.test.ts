import { describe, expect, it } from "vitest";
import {
  MAX_QUANTITY_HUNDREDTHS,
  MAX_UNIT_PRICE_CENTS,
  centsToInput,
  formatCents,
  formatQuantity,
  lineAmountCents,
  parsePriceToCents,
  parseQuantityToHundredths,
} from "./money";

describe("formatCents", () => {
  it("groups thousands and always shows 2 dp", () => {
    expect(formatCents(0)).toBe("0.00");
    expect(formatCents(5)).toBe("0.05");
    expect(formatCents(100)).toBe("1.00");
    expect(formatCents(123456)).toBe("1,234.56");
    expect(formatCents(100_000_000)).toBe("1,000,000.00");
    expect(formatCents(-123456)).toBe("-1,234.56");
  });

  it("is exact at the largest possible line amount", () => {
    expect(formatCents(1_000_000_000_000_000)).toBe("10,000,000,000,000.00");
  });
});

describe("parsePriceToCents", () => {
  it("accepts whole numbers, 1–2 dp, commas and a leading $", () => {
    expect(parsePriceToCents("400")).toBe(40000);
    expect(parsePriceToCents("400.5")).toBe(40050);
    expect(parsePriceToCents("400.05")).toBe(40005);
    expect(parsePriceToCents("1,200.00")).toBe(120000);
    expect(parsePriceToCents(" $75 ")).toBe(7500);
    expect(parsePriceToCents(".5")).toBe(50);
    expect(parsePriceToCents("0")).toBe(0);
    expect(parsePriceToCents("10,000,000.00")).toBe(MAX_UNIT_PRICE_CENTS);
  });

  it("rejects negatives, >2 dp, garbage and too-large values", () => {
    for (const bad of [
      "",
      " ",
      ".",
      "-1",
      "1.234",
      "abc",
      "1.2.3",
      "1e3",
      "10000000.01",
      "999999999999",
    ]) {
      expect(parsePriceToCents(bad), bad).toBeNull();
    }
  });
});

describe("centsToInput", () => {
  it("drops .00 and keeps two digits otherwise", () => {
    expect(centsToInput(40000)).toBe("400");
    expect(centsToInput(12550)).toBe("125.50");
    expect(centsToInput(5)).toBe("0.05");
  });

  it("round-trips through parsePriceToCents", () => {
    for (const cents of [0, 1, 99, 100, 12550, MAX_UNIT_PRICE_CENTS]) {
      expect(parsePriceToCents(centsToInput(cents))).toBe(cents);
    }
  });
});

describe("quantity", () => {
  it("parses like the API", () => {
    expect(parseQuantityToHundredths("2")).toBe(200);
    expect(parseQuantityToHundredths("1.5")).toBe(150);
    expect(parseQuantityToHundredths("0.25")).toBe(25);
    expect(parseQuantityToHundredths(".5")).toBe(50);
    expect(parseQuantityToHundredths("1000000")).toBe(MAX_QUANTITY_HUNDREDTHS);
  });

  it("rejects what the API rejects", () => {
    for (const bad of [
      "",
      "0",
      "-1",
      "1.234",
      "1,5",
      "1.",
      "two",
      "1000000.01",
    ]) {
      expect(parseQuantityToHundredths(bad), bad).toBeNull();
    }
  });

  it("formats minimally", () => {
    expect(formatQuantity(200)).toBe("2");
    expect(formatQuantity(150)).toBe("1.5");
    expect(formatQuantity(25)).toBe("0.25");
    expect(formatQuantity(1050)).toBe("10.5");
  });
});

describe("lineAmountCents", () => {
  it("rounds half up", () => {
    expect(lineAmountCents(200, 40000)).toBe(80000);
    expect(lineAmountCents(150, 333)).toBe(500);
    expect(lineAmountCents(25, 1)).toBe(0);
    expect(lineAmountCents(50, 1)).toBe(1);
  });

  it("does not lose precision at the bounds", () => {
    expect(lineAmountCents(MAX_QUANTITY_HUNDREDTHS, MAX_UNIT_PRICE_CENTS)).toBe(
      1_000_000_000_000_000,
    );
  });
});
