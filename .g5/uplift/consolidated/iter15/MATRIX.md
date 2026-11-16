# Iteration 15 observations

| Boundary | Observed result |
|---|---|
| + to _, __ platform codec | Exact native prerelease/build roundtrip succeeds |
| v1 current, v2 incompatible | Normal row/install select v1; callable payload1 |
| Service AllVersions0 | One preferred v1 row with current platform |
| Service AllVersions1 | One preferred v1 row; raw all-tags text; no platform list |
| SQL show-all / manager on that grouped row | Display shows row; manager filters it |
| Empty AllVersions | Service emits v2bad,v1current; manager retains v1 and current pull tag |
| Range >=1 consumer | Consumer/dependency rows present; payload1 |
| Pinned ^2 consumer | Specific satisfactory-version failure; both rows/payload absent |
| v1 current+bad platform tags | Current tag selected; payload1 |
| Shallow build a.9 / z.1 | Decoded and encoded-key order agree |
| Deep build a.1.a / a.1.z | Encoded comparison fields equal; aggregate A first, decoded Z first |
| Actual two-build publications | Source/package versions and raw tags both qualified |
| Normal service/SQL listing after publications | A |
| Dependency service / manager / actual install | A,Z / Z / Z |

The install-A hypothesis was refuted. A successful package-status-only producer attempt did not prove artifact identity and is retained as a miss. Final full source-manifest publication establishes both tags. No Windows/mirror/restart/concurrent registry mutation or generic parser test is claimed.
