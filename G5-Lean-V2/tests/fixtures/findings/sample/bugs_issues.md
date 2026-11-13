# Bugs and issues

Found by the verification run of 1 November 2026; not fixed.

## Actionable

<a id="rcv-001"></a>
### Duplicate receipt lines bypass the receiving ceiling

Severity: Critical · Decision: Open

Two lines for one PO item each pass the ceiling alone. See [receipt-ceiling](../ontology/features/receiving.g5.md#receipt-ceiling) and [the run](results/receiving/20261024T012111Z-asbuilt/receipt.json).

#### Proposed repair

Aggregate surviving lines per PO line inside the submit transaction.

<a id="rcv-002"></a>
### A stale draft can receive against a closed purchase order

Severity: High · Decision: Need fix — [ACME-12](https://g5.example/acme/issues/12)

Submit does not recheck the PO's state.

<a id="rcv-003"></a>
### Receipt switch skips the ownership check

Severity: Medium · Decision: Defer — [acme/api#7](https://github.com/acme/api/issues/7)

Switching the PO reuses only the status check.

<a id="rcv-004"></a>
### Receipt numbers restart after a settings save

Severity: Low · Decision: Irrelevant — user decided this issue is not important. Closed [ACME-14](https://g5.example/acme/issues/14).

```markdown
<a id="not-an-entry"></a>
### A heading inside a code example
```

<a id="rcv-005"></a>
### The reminder clock uses the wrong calendar

Severity: Unranked · Decision: Irrelevant — user decided this issue is not important.

<a id="rcv-006"></a>
### Imports accept a negative quantity

Severity: Severe · Decision: Open

The severity is not one of the four levels.

<a id="rcv-007"></a>
### Exports ignore the company filter

Severity: Low · Decision: Maybe later

<a id="rcv-008"></a>
### Escalation counts weekends

Severity: Low · Decision: Need fix

The decision names no issue.

## Other entries

<a id="rcv-q-001"></a>
### Whether recall after approval is allowed is undecided

Severity: Medium

An open intent question, not a defect.

<a id="rcv-q-002"></a>
### The knowledge article describes the old toolbar

The status line is missing.
