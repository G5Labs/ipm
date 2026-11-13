# Runtime proposal for the security owner

This is exact proposed canonical content for root to integrate into `concepts/security.g5.md`; it is not itself a governing block. Agent recovery of C04, based on SEC-002's recorded human instruction, scoped interpretation, merged `67ad4b5d20ac1a291382733854292fa42a494fdf` and the active MVP scope amendment. Do not restore the deferred general organization egress-policy system.

```markdown
<a id="g5.security.guest-egress"></a>
[D · g5.security.guest-egress]

Before customer execution, Linux microVM networking enforces denial of cloud metadata, link-local and unintended host-local access. Preserve required DNS, narrowly configured cache/API access and replies to host-initiated connections. Enforcement failure prevents boot. Retain guest-local networking through the per-VM TSI translation and the explicit trusted process/macOS development exceptions. The policy remains installed during execution, assuming trusted external firewall administration; an external ruleset flush is not automatically repaired until another boot. This is not general private-network isolation.
```

Footer:

```markdown
### g5.security.guest-egress

- agent
- Source: [SEC-002 human instruction and scoped interpretation](../../../chat_intent/sec-002/guest-egress.md), merged commit `67ad4b5d20ac1a291382733854292fa42a494fdf`; [active scope amendment](../../../../docs/v9-mvp-scope-amendment.md). The broader organization-controlled egress system remains deferred.
- Validation: [runtime coverage models](https://github.com/G5Labs/G5-Lean/blob/072a12d3e99032ee3a7356837542c5edeb825984/.g5/TLA_verification/ben-coverage/runtime/README.md), GuestEgress. The finite policy model checks installation ordering, failure closure, representative denied destinations and required exceptions. Kernel packet behavior, the Linux TSI patch, external firewall changes and development-mode isolation are outside this model; no runtime/network test was run in this refinement.
```

The body preserves the previously accepted TSI and external-firewall qualifications from historical ontology; this refinement does not claim a new complete audit of the TSI patch. The finite representative traffic set deliberately permits DNS and established replies, so “denial” is subject to the stated exceptions rather than an unqualified assertion about every packet to an address.
