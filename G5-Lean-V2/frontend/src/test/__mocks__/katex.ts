// Stub of the katex renderer for unit tests: rendered math shows as its TeX
// source, so tests can assert what was rendered without KaTeX's output.
export default {
  renderToString: (tex: string, _options?: Record<string, unknown>) =>
    `<span class="katex-mock">${tex}</span>`,
};
