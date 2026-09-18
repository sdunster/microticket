// Registers toBeInTheDocument and friends for every test file. Without it each
// file has to import it itself, and one that forgets fails with the unhelpful
// "Invalid Chai property: toBeInTheDocument" rather than anything pointing at
// the missing import.
import "@testing-library/jest-dom/vitest";
import { vi } from "vitest";

// jsdom doesn't reliably provide a working `localStorage` under recent Node
// versions (Node's own experimental global `localStorage` shadows it and
// throws without a `--localstorage-file` flag) — swap in a plain in-memory
// implementation so `lib/sessionToken.ts` and the instance switcher's
// persistence work the same in tests as in a real browser.
const localStorageMock = (() => {
  let store: Record<string, string> = {};
  return {
    getItem: (key: string) => store[key] ?? null,
    setItem: (key: string, value: string) => {
      store[key] = String(value);
    },
    removeItem: (key: string) => {
      delete store[key];
    },
    clear: () => {
      store = {};
    },
    key: (index: number) => Object.keys(store)[index] ?? null,
    get length() {
      return Object.keys(store).length;
    },
  };
})();

vi.stubGlobal("localStorage", localStorageMock);
