import { describe, expect, it } from "vitest";
import { formatDate, localToday } from "./dates";

describe("localToday", () => {
  it("uses local calendar fields, zero-padded", () => {
    expect(localToday(new Date(2026, 7, 9, 23, 59))).toBe("2026-08-09");
    expect(localToday(new Date(2026, 0, 1, 0, 0))).toBe("2026-01-01");
  });

  it("defaults to now", () => {
    expect(localToday()).toMatch(/^\d{4}-\d{2}-\d{2}$/);
  });
});

describe("formatDate", () => {
  it("formats as DD/MM/YYYY without timezone shifts", () => {
    expect(formatDate("2026-08-19")).toBe("19/08/2026");
    expect(formatDate("2026-01-01")).toBe("01/01/2026");
  });

  it("passes anything else through", () => {
    expect(formatDate("not a date")).toBe("not a date");
  });
});
