# Evaluator corrections and scope

The original twelve Rust test functions remain unchanged and all raw executions are
retained. These corrections change fixture representation or request transport, not
the promised application outcomes. None was sent to experimental workers.

## Successful-command control

The protocol's permitted-behavior controls include reusing an already successful
command/bootstrap prerequisite without executing it again. The first additional probe
used an informal command specification. Before the comparable final runs, v2 changed
that fixture to the repository's typed `workspace.bootstrap` envelope and removed its
agent chat/input association. Both baseline versions passed. The original is retained
as `permitted-prerequisite-probe.v1.rs`; `permitted-freeze-v2.json` records the correction.
The same success, three-agent-task and unchanged-source assertions remain.

## Newly required preview fingerprint

After u06 completed, the frozen backend fixture failed before reaching eleven of its
twelve assertions: the candidate requires a `sourceFingerprint` that the old request
does not supply. The UI mock similarly omitted this field, so u06 could not reload the
mock's saved request. The original successful-command control also failed at request
deserialization. These raw failures do not establish eleven independent defects, failed
recovery with a real preview, or rejection of successful command prerequisites.

`compatible-freeze.json` records the separate normalization before its first execution.
`evaluate_compatible.py` retains every original assertion. It obtains an optional
opaque fingerprint through the public preview producer, and includes it when supplied.
After an evaluator deliberately modifies a valid source graph for the cap/command
control, it refreshes that preview before the first submission. Repeated submissions
reuse their saved body; no fresh preview is substituted during recovery. The UI mock
adds a valid-shaped fingerprint. Baseline and every delivery receive this same adapter.

The normalized backend suite combines the original twelve functions with the separate
successful-command control, so its denominator is thirteen. Report the control separately
when comparing the original twelve outcomes. Its additional public-preview precondition
is a limit; some negative cases already accept any client error and do not identify the
precise refusal reason. No adapter proves backward compatibility: existing saved requests
and receipts without fingerprints require separate adjudication against the governing
recovery promise. The study does not silently treat API changes as either permitted or
forbidden simply to improve test counts.

## Other execution limitations

The u01 supplemental Rust check briefly overlapped the end of the primary check in the
same evaluator build directory. Cargo serialized builds; each TestApp uses its own UUID
database. Results are retained, and timings include contention. Later evaluator suites
run serially. Worker frontend commands that initially missed the private environment
are recorded as deviations; a subsequent correctly configured run does not erase them.
