# G5 on G5 with VM isolation: the full-stack preview's host in microvm mode

## Human request

On 2026-10-28 the owner (Kaihui) wrote in this workspace's chat: "read the request doc and
implement it, if you have any questions, ask me before making a decision." The request doc
is the Project Doc `request.md` (workspace `3927fd16-a974-437f-8185-a653ea04a22d`), a
follow-up to the nested-virtualization work merged as PR #853.

Its goal, as written: a workspace VM running the `full-stack` preview profile starts its
previewed worker host in the default `microvm` isolation mode, so the previewed G5
instance boots real Cloud Hypervisor guests inside the workspace. Process mode stays
available for macOS and ARM, unchanged. Owner decisions already made in the doc: option A
(host-supplied platform artifacts on the runtime disk), not baking a kernel into g5dev and
not a prebuilt-download path; nested guest boot time is not a concern. The doc lists seven
scope items for one PR (guest-kernel Landlock, platform on the runtime disk, discovery in
`scripts/worker-host-env.sh`, guest-image accounts and nftables, the preview profile, the CI
gate, the ontology amendment), a verification plan and acceptance criteria.

## Agent interpretation

- The saved `request.md` is damaged: about twenty lines lose a span of text in the middle
  (the single commit of the file in the docs repository has the same bytes). The design
  reconstructs those spans from the surrounding words and the code; Q4 of the question set
  asks the owner to confirm the reconstruction or re-save the document.
- "Ask me before making a decision" means the design and its decisions are presented first,
  through a Project Doc question set, and implementation starts when the owner answers.
  Choices the request explicitly delegates ("the design decides") are decided in the design
  and listed for reading, not asked.
- Investigation found three conditions the request does not list but that its goal depends
  on (recorded as facts in the design): the guest kernel cannot install the host's passt
  egress policy (no `inet` nf_tables family, no `ct`/`fib` expressions), the guest's ext4
  workload disk has no reflink while a microvm host requires one at admission, and the
  inner host needs a Linux guest runtime that `just dev-up` does not stage. These change
  the kernel and profile scope and are the subject of the owner questions.

## Continuation (2026-10-28)

After the design and the question set were presented, the owner wrote "continue" with
no answers recorded. Interpretation: proceed with the implementation on the recommended
options, record them on the question set as agent proposals (revision 2 of each
question; not decisions until the owner confirms them), and keep the design adjustable
should the owner choose otherwise. Q4 (the damaged request document) stays open for the
owner.
