# Script preparation and execution neighbors

All cases reset actual DefaultLogEntryLimit to7 and the runtime counter to0. First/last writes are101/202.

| File case | Dry | Status | Final value | Runtime counter |
|---|---:|---:|---:|---:|
| valid text / valid JSON | 0 | OK | 202 | 0 |
| text middle unknown command | 0 | error | 101 | 0 |
| JSON middle unknown command | 0 | error | 7 | 0 |
| text / JSON known runtime error | 0 | error | 101 | 1 |
| text middle unknown command | 1 | OK | 7 | 0 |
| valid JSON | 1 | OK | 7 | 0 |
| JSON middle unknown command | 1 | error | 7 | 0 |
| root array / wrong commands shape | 0 | error | 7 | 0 |
| missing file / .text extension | 0 | error | 7 | 0 |

Actual GetValue and direct `%SYS` global readers agree. Text dry-run lists the last command without executing it. Text runtime/unknown failures and JSON runtime failure stop before listing the last command; JSON unknown preflight failure lists no first command. The preview header is a separate rendering gate. The known exec fixture increments an actual job-local counter before division by zero; it is a qualified child interpreter effect, not a proof of ObjectScript semantics. All86 native fields have expected gates.
