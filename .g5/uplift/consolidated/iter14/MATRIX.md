# Iteration 14 observations

All module loads/uninstalls succeed; loaded row1 becomes row0 after uninstall. All actual processors report Overlay0/Keep0. Payload bytes match after copy.

| Scratch target | Classifier before load | Copy result | Clean result |
|---|---|---|---|
| Ordinary directory | Unprotected | Payload copied; unrelated sentinel retained | Directory removed |
| Exact synthetic IRIS.DAT | Protected | Payload and sentinel/marker remain | Directory retained |
| Marker added after ordinary load | Unprotected then protected | Sentinel retained | Payload/directory retained |
| Marker removed after protected load | Protected then unprotected | Sentinel retained | Directory removed |
| Single-file resource inside marker directory | Protected directory | Payload copied | Payload removed; directory/marker/sentinel retained |
| Parent of unknown synthetic child marker | Unprotected | Nested marker/sentinel retained | Whole parent removed |
| Direct Unix CopyDir flag0 / flag1 | N/A | Both replace same-name payload and retain sentinel | N/A |

The expected ordinary/nested copy clearing was refuted, not reclassified as a successful prediction. System-root/ancestor/child/raw normalization/case and USER database-list membership are separate read-only calibrations. No Windows, real database removal or restart test occurred.
