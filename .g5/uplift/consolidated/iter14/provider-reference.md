# CopyDir provider qualification

Pinned native signature is in the final session log: pSource,pTarget,pOverlay=0,&pCreated,pDeleteBeforeCopy=0. Direct flag0/1 calibration overwrites an old same-name payload and preserves an unrelated sentinel. The IPM Unix call fixes pOverlay1 and forwards its deletion flag to pDeleteBeforeCopy.

The [official File class reference](https://docs.intersystems.com/irislatest/csp/documatic/%25CSP.Documatic.cls?CLASSNAME=%25Library.File&LIBRARY=%25SYS#CopyDir) describes the last argument as deletion before replacing matching files, while pOverlay permits an existing target. It was consulted2026-11-15 and labels itself2026.2; the pinned2026.1 native signature and results, not a latest-version assumption, support this experiment. Generic OS errors and cross-platform behavior remain outside native qualification.
