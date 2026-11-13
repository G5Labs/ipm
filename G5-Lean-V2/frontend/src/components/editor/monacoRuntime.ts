// =============================================================================
// G5 Editor — monacoRuntime
// =============================================================================
// Monaco ships with the G5 frontend (`g5.documents.editor.assets`): without
// this, `@monaco-editor/react` fetches Monaco from cdn.jsdelivr.net at
// runtime, so an install without internet access could not edit and the
// editor version would follow the loader's default instead of package.json.
// Importing this module points the loader at the bundled copy and gives
// Monaco its web workers from the same build. Editors import it lazily,
// together with `@monaco-editor/react`, so pages that never show code do not
// load it.
// =============================================================================

import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/esm/vs/editor/editor.worker?worker";
import JsonWorker from "monaco-editor/esm/vs/language/json/json.worker?worker";
import CssWorker from "monaco-editor/esm/vs/language/css/css.worker?worker";
import HtmlWorker from "monaco-editor/esm/vs/language/html/html.worker?worker";
import TsWorker from "monaco-editor/esm/vs/language/typescript/ts.worker?worker";

self.MonacoEnvironment = {
  getWorker(_workerId: string, label: string): Worker {
    switch (label) {
      case "json":
        return new JsonWorker();
      case "css":
      case "scss":
      case "less":
        return new CssWorker();
      case "html":
      case "handlebars":
      case "razor":
        return new HtmlWorker();
      case "typescript":
      case "javascript":
        return new TsWorker();
      default:
        return new EditorWorker();
    }
  },
};

loader.config({ monaco });

export { monaco };
