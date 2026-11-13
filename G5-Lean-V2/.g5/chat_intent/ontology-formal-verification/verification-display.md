# Ontology verification display

## Human requests and decisions (2026-10-31)

The owner supplied an HTML mock of a customer project, the pilot repository, and a handoff document of earlier UI instructions,
then an artifact organization proposal, as Project Docs in this workspace
(`ontology-verification-ui-handoff.md`, the HTML mock,
`verification-artifact-organization-proposal.md`). The reviewed design is
`ontology-verification-display-design.md` there; its open decisions were answered in the
question set `verification-display`. At PR time the owner chose to keep the mock out of
the repositories, because it embeds customer content; the two Markdown documents are
committed.

The owner asked to build the mock's ontology display into G5:

1. Only the ontology page; the Bugs/issues and Design simplification pages, and their
   sections in verification details, wait for a later session. The root page and sidebar
   are out of scope.
2. Display the verification evidence associated with ontology statements, with the
   mock's bullets and fold/unfold.
3. Treat it as an incremental update of the current Core Ontology viewer. The existing ⓘ
   metadata button stays; minor improvements from the mock may be adopted.
4. A statement's status comes from its metadata tag, at most one, with `future` ignored.
   No tag shows Unverified; Unverified shows on every statement, quietly.
5. Design verified holds only when the tag is present and the verification details can be
   found; otherwise the statement shows Unverified. Staleness does not matter for now.
6. A harness helper script copies links from state machine manifests into metadata and
   marks linked statements design-verified. It does nothing else; it replaces a weaker
   verification tag but never `implementation-verified`.
7. A parent with a design-verified child shows Design verified, with a note naming the
   children; a child's button shows its own status.
8. A child is a statement whose ID extends its parent's and that sits immediately after
   the parent or the parent's earlier children. A closed fold reveals nothing.
9. Remove "Set a different verification status for future".
10. Source-evidence links in metadata must be clickable, with robust path handling.
11. The artifact guide is a separate file referenced by the uplift and downlift guidance,
    and also by planning's model-checking reference.
12. Keep the strict tag rule (no trailing period or note).
13. Extra anchor lines directly above a statement's anchor are aliases for former IDs.
14. "yes please": implement the reviewed design.

## Owner feedback on the preview (2026-11-01)

15. The dot of a statement with nothing to open was too small; it should be only slightly
    smaller than the triangle.
16. Unverified shows nothing when opened ("If it is unverified, no need to show
    anything"). This supersedes the earlier design, in which details named the missing
    evidence.
17. Test Coverage and Manual checking and review show the details in the statement's
    metadata about it.

## Agent interpretation

- Decisions 16 and 17: Unverified and Checking… are plain labels with no details.
  Implementation verification is treated like decision 17, since the viewer reads no
  evidence for it. "The details in the metadata" are the statement's metadata entry
  without its bare tag bullets and `Expands:`/`Children:` lines. Linked state machines
  show only for a design-verified tag.

- Answers 5 and 6 are read together: the script tags statements whose links reach a state
  machine with a manifest and a recorded run; it does not judge outcomes, remove anything
  or tag parents.
- "Verification details can be found" means a linked state machine folder with a
  `manifest.json` and a row for it in the run index.
- The handoff's typography (18px text, 24px triangles, 22px dots, black markers) and its
  panel layout ("[title]: What this state machine checks" outside the fold) carry over.
- Because the tag rule stays strict, the pilot repository's `- agent.` bullets stay unrecognized as
  provenance until rewritten.
- The handoff names `.g5/TLA_verification/` the workspace-visible artifact location. G5
  keeps it out of the Files tree, but Save commits it and the file API reads it, so the
  viewer reads linked artifacts directly and the panel opens their files. Listing it in
  Files is left open: the pilot repository's roughly 56,000 artifact entries would push most of the
  repository out of a tree that loads at most 10,000.

