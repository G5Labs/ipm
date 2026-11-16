# Selected developer source-file neighbors

| Module/layout | Operation | Mode | Live class map | Fresh declared item path | File exists |
|---|---|---|---|---|---|
| implicit fallback | normal install | 0 | absent | src/cls/Iter25/Value.cls | 0 |
| implicit fallback | developer reinstall | 1 | absolute src/ | src/cls/Iter25/Value.cls | 0 |
| implicit fallback | normal reinstall refused | 1 | unchanged | unchanged | 0 |
| explicit Directory=code | normal install | 0 | absent | src/code/Iter25/Explicit.cls | 1 |
| explicit Directory=code | developer reinstall | 1 | absolute src/code/ | src/code/Iter25/Explicit.cls | 1 |
| explicit Directory=code | normal reinstall refused | 1 | unchanged | unchanged | 1 |

Both loaded classes remain callable with payload1. Explicit paths are opened and the actual class/method source is read. The MODULE wildcard NoFolders becomes1 on the first developer reload and remains1 through the second module's normal state; module-specific mappings are independent. Typed mappings contain char8 absolute-prefix markers and exact selected directory bytes. Product metadata/path readers do not consume those maps. External modern source-control integration remains unassessed. Legacy SDK empty-reader results are retained as failed reader predictions, not product defects.
