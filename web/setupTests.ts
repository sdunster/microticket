// Registers toBeInTheDocument and friends for every test file. Without it each
// file has to import it itself, and one that forgets fails with the unhelpful
// "Invalid Chai property: toBeInTheDocument" rather than anything pointing at
// the missing import.
import "@testing-library/jest-dom/vitest";
