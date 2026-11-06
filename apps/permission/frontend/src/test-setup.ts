import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/svelte";
import { afterEach, vi } from "vitest";

window.matchMedia = ((media: string) => ({ matches: false, media, addEventListener() {}, removeEventListener() {} })) as unknown as typeof window.matchMedia;
Element.prototype.scrollIntoView = () => {};

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});
