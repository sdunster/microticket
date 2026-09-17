import { describe, expect, it } from "vitest";
import { tw } from "./tw";

describe("tw", () => {
  it("returns the template string unchanged", () => {
    expect(tw`flex items-center gap-2`).toBe("flex items-center gap-2");
  });

  it("interpolates values like a normal template literal", () => {
    const size = "lg";
    expect(tw`text-${size} font-bold`).toBe("text-lg font-bold");
  });

  it("preserves multiple interpolations in order", () => {
    const a = "p-4";
    const b = "m-2";
    expect(tw`${a} ${b} flex`).toBe("p-4 m-2 flex");
  });
});
