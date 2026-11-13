# View images and PDFs from FILES — 2026-10-31

## Human instructions

The workspace owner assigned [issue #982](https://github.com/G5Labs/G5-Lean/issues/982)
("Support viewing more file types via FILES tab"), whose two comments of 2026-10-31
recommend an image viewer built on the browser's own decoding and a PDF viewer built on
Mozilla PDF.js, each with its scope, controls, operating limits and acceptance conditions.
In chat the owner wrote:

> there are one comments to support viewing image and the other comment to support pdf,
> please build both features

> after reading the issue to get the context, please follow the process for design and
> then proceed on implementation.

> once you verified your changes, please file a pr and then follow instructions below to
> make the pr ready to ship

## Agent interpretation

Both recommendations are the product decision; the design is authorized and so is its
implementation, without a further approval step. The issue's comments are the durable
source of the behaviour; the [plan](../../pr_ontology_diff_design/support-viewing-image-and-pdf-via-files-tab/ontology-plan.md)
records the choices the comments left open (the pinned PDF.js build, what counts as a
supported image, the decoded-size bounds, what the viewers do not do yet) and the
evidence. The issue's closing question, a Download button for unsupported types, is
already answered by the existing card and stays as it is.
