# Native byte neighbors

| Stored status / mode | Summary | Details before ANSI | UTF-8 observation |
|---|---|---|---|
| Finalized success | literal Success | raw scalar 1 | summary/details valid |
| No-repository failure | decoded error text | 311 raw status bytes | summary valid, details invalid |
| Checked Invoke failure with CR/LF/TAB | decoded error text with visible labels | 657 raw bytes become 667 bytes with visible labels | classified from original bytes, no generic text promise |
| Same three records, ColorScheme none | same selected content | same uncolored content/labels | ANSI removal does not change raw-status encoding |

Raw status is required by history.read.details; prior B2 D7/R1-ONT-13 withdrawal remains. Encoding/IsOK/GetErrorText and the actual setting are provider inputs, never a final rendered-answer premise. The native observer preserves exact bytes before escaped text conversion. These are known builder cases, not independent heldouts.
