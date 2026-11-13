# Security invariants: the two-tier threat model

Source: the owner's Project Doc `threat-model.md` (docs repository,
`docs/d09348e2-1a56-47b3-b0bb-4704d0ce4641/threat-model.md`, docs commit `f258af2`),
written by the owner, Zhengchun Liu, on 2026-10-30 after a meeting with James Moore on
2026-10-29. On 2026-10-31 the owner asked this workspace to work on that document. The
authorized code branch is `threat-model-as-invariants`, started at `c351b446c`.

The document defines an ontology task only: edit pages under `.g5/ontology/`, change no
code, and where the code does not yet meet a statement record a gap in that block's
metadata rather than softening the statement. Every decision needed was made by the owner
and is listed below; the document says not to reopen them. The three subsections under
"Human instructions" reproduce the document's sections 1 to 3 verbatim.

## Human instructions

### The owner's threat model, verbatim

> **Tier 0 threat model, across orgs.** G5 is a multi-tenant SaaS application. Tenancy is implemented via Row-Level Security in the database along with separation of computation across tenancy. No two org microVM share the same hypervisor. The row level security block policies block cross organization reads.
> The term "Nobody" below is defined as people from other organizations, people who were removed (from the moment of removal, including connections they already had open), and machines belonging to another organization (workspace VMs, API keys, worker hosts, service accounts).
>
> - Nobody uses G5 without signing in with a verified identity provider (Github and Entra).
> - Nobody sees or changes an organization's data or uses its compute (host and microVM) unless they are a current member of that organization.
> - Nobody can sign in G5 without an org membership.
> - A worker host only have access to org-level credentials and secrets.
> - A worker host does not have network access to other hosts, and have no access to other GCP resources.
> - All secrets must be encrypted at rest and in transit.
> - Customer data must be encrypted at rest.
>
> **Tier 1 threat model, authorized users within an org.**
>
> - Nobody creates a record that looks like another person made it (commit, chat message, comment, vote, approval, GitHub write under someone else's account). Through G5 API, and G5 minted credentials, identity is tied to an individual and cannot be forged or impersonated.
> - Nobody writes into an object (git, chat history and settings) that belongs to someone else (another owner's branches).
> - Only organization Admin can change org scoped settings.
> - Nobody sees what they are not authorized to access.
> - Nobody runs their own code on worker hosts or inside G5 itself, whether by escaping a VM or by tampering with what we build and ship.
> - CI/CD pipeline must include Common Vulnerabilities and Exposures scan for all images to be deployed.
> - Customer secrets accessible by a workspace are visible as environmental variables to agents.
> - Write down the permission model: Admin can transfer ownership of any workspace to anybody. Anyone can read branch as the basic permission for each org member is a viewer of the workspace.

### Decisions the owner has made (2026-10-30)

1. **"Nobody runs their own code on hosts or in G5" and the CVE scan stay on tier 1**, as written. Do not move them to tier 0.
2. **Commit author and committer text is advisory.** The recorded push (which workspace pushed, and its owner) is the authoritative attribution. G5 does not verify the text at receive.
3. **The CVE requirement covers every image G5 deploys**: the application container images, the worker host boot image and the platform guest images. Today only the application containers are scanned; the other two are a gap.
4. **The worker hosts' shared cloud identity is accepted as the design.** All hosts of all organizations run as one identity per deployment that can only write logs and metrics and read the guest compile-cache prefix. This is not a gap; state it as the design and its reason.
5. **Every active member reads every ordinary workspace.** Invite-only workspaces are retired until a future private-workspace feature protects records and Git branches together. Formal review workspaces (human read-only) and personal workspaces (owner and administrators only) keep their narrower rules. The code still offers invite-only; retiring it is a gap, not something to do in this task.

### Points from the 2026-10-29 meeting

- **A VM breakout will happen eventually.** The microVM was adopted for dependency isolation, not as a security boundary, and tenancy must not rest on it. Treat a breakout as critical in severity and medium-to-high in likelihood. The hard rule that bounds it: a compromised worker host exposes only secrets and credentials tied to its own organization.
- **A worker host may reach the internet.** That is allowed and intended.
- **Vulnerability management is regular, proactive monitoring**, not only a scan at deploy time. Images are re-checked as new vulnerabilities are published.
- **Define "the ledger"**: every durable record that names a person as its actor: Git history, chat history, comments, suggestions, votes, approval decisions, issue writes, and the writes G5 makes at an external provider such as GitHub.
- **Impersonation wording**: through the G5 API and G5-vended credentials, identity is tied to an individual and cannot be forged or impersonated.
- **Merging a child into its parent writes the parent's branch.** The meeting left this open. The code answers it: only the parent's current owner may land the merge (an administrator may do it for Base, which has no owner). The child's owner cannot push into the parent.
- **"Customer data encrypted at rest" means the cloud provider cannot read repositories**, which needs customer-managed encryption keys for the stores that hold customer data.
- **Secrets visible to agents as environment variables are an accepted, unmitigated threat**: an agent can read a secret or leak it into chat, and the owner can read it. G5 declares this as the service's behavior rather than hiding a secret from the person whose VM runs with it.
- **Customer content must never appear in error logs**, or in telemetry or error messages. Logs carry identifiers and reasons, not repository content, chat text, document text or secret values.
- **Defense in depth on tenancy**: every SQL query is written tenant-scoped, and row-level security is a second, independent mechanism. No single mechanism is trusted alone.

### Owner decision (2026-11-06)

The ownership invariant's parent-merge rule contradicted the source-owner merge that #1104 added on 2026-11-05. The owner chose to reword the invariant to name that path:

> (a) Reword the invariant to name the source-owner path: "…lands through a G5 merge by someone with merge authority (the parent's owner, an administrator for Base, or the child's owner under source-owner merge); nobody pushes into the parent".

## Interpretation

- The document's section 4 names each block, its order, marker, provenance and content;
  its section 5 lists the code facts the blocks may rely on. The
  [design](../../pr_ontology_diff_design/security-invariants/ontology-plan.md) records how
  the edits apply it, where the prose is the agent's and which facts were re-checked.
- Two of the section 5 facts were narrowed on re-reading the code at `c351b446c`: the
  deploy pipeline's CVE report already covers the guest environment images, not only the
  application containers, and G5 keeps no record dedicated to pushes. Neither weakens a
  statement; both are recorded as gaps in the metadata of the blocks they qualify.
- Scope: no code change, no implementation of the recorded gaps, no TLA+/TLC work. The
  follow-up work the document lists in its section 7 is carried in the design, not done
  here.
