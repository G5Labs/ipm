import { describe, expect, it } from "vitest";
import { pdfAssetBase, pdfDocumentOptions } from "./pdfjs";

// `g5.documents.viewers.pdf`: the build, worker and assets come from G5's own
// versioned asset path, named by the pinned package through the Vite plugin.
describe("pdfjs assets", () => {
  it("resolves the pinned version's asset path", () => {
    expect(pdfAssetBase()).toMatch(/^\/assets\/pdfjs\/\d+\.\d+\.\d+\/$/);
    const options = pdfDocumentOptions();
    expect(options.cMapUrl).toBe(`${pdfAssetBase()}cmaps/`);
    expect(options.standardFontDataUrl).toBe(
      `${pdfAssetBase()}standard_fonts/`,
    );
    expect(options.wasmUrl).toBe(`${pdfAssetBase()}wasm/`);
    expect(options.iccUrl).toBe(`${pdfAssetBase()}iccs/`);
    expect(options.cMapPacked).toBe(true);
    expect(options.enableXfa).toBe(false);
  });
});
