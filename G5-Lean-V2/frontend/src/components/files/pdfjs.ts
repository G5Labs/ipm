// =============================================================================
// G5 Files — pdfjs
// =============================================================================
// How the PDF viewer gets Mozilla PDF.js (`g5.documents.viewers.pdf`): the
// pinned `pdfjs-dist` compatibility build and its worker, imported at runtime
// from G5's own hashed assets, where the `g5-pdfjs-assets` plugin in
// vite.config.ts also puts the character maps, standard fonts, ICC profiles
// and image-decoder WebAssembly the worker fetches on demand. PDF.js is
// self-contained, so nothing of it is bundled, and nothing here loads the
// scripting sandbox, XFA or the annotation layer.
// =============================================================================

import type * as PdfJs from "pdfjs-dist/legacy/build/pdf.mjs";
import { version } from "virtual:g5-pdfjs";

export type PdfJsModule = typeof PdfJs;
export type {
  PDFDocumentLoadingTask,
  PDFDocumentProxy,
  PDFPageProxy,
  PageViewport,
  RenderTask,
  TextLayer,
} from "pdfjs-dist/legacy/build/pdf.mjs";

/** Where the emitted PDF.js build, worker and assets are served from. */
export function pdfAssetBase(): string {
  return `${import.meta.env.BASE_URL}assets/pdfjs/${version}/`;
}

let runtime: Promise<PdfJsModule> | null = null;

/** Loads PDF.js from its asset path and points it at its worker, once. */
export function loadPdfJs(): Promise<PdfJsModule> {
  runtime ??= import(/* @vite-ignore */ `${pdfAssetBase()}pdf.min.js`).then(
    (pdfjs: PdfJsModule) => {
      pdfjs.GlobalWorkerOptions.workerSrc = `${pdfAssetBase()}pdf.worker.min.js`;
      return pdfjs;
    },
    (error: unknown) => {
      // A failed load is retried on the next open.
      runtime = null;
      throw error;
    },
  );
  return runtime;
}

/**
 * The fixed `getDocument` options: G5-served assets, no XFA. Document
 * scripting needs a scripting manager this viewer never creates.
 */
export function pdfDocumentOptions(): {
  cMapUrl: string;
  cMapPacked: boolean;
  standardFontDataUrl: string;
  iccUrl: string;
  wasmUrl: string;
  enableXfa: boolean;
} {
  const base = pdfAssetBase();
  return {
    cMapUrl: `${base}cmaps/`,
    cMapPacked: true,
    standardFontDataUrl: `${base}standard_fonts/`,
    iccUrl: `${base}iccs/`,
    wasmUrl: `${base}wasm/`,
    enableXfa: false,
  };
}
