# Matching Source Control files — 2026-10-25

The owner requested hiding nonmatching line-by-line diffs and keeping the result current when either the selected commits or file categories grow or shrink. They proposed showing only matching filenames as well, removing the greying-out behavior.

The adopted interpretation is one matching file set for both the tree and detailed diffs, derived from the applied commit comparison and file categories. Existing Show Changes application, pending edits, Save reconciliation and access withdrawal remain in force. Selecting no categories gives an empty matching view; selecting a category or additional commits restores the corresponding files on apply.

The owner also requested checking that both the filename list and actual diff rendering remain efficient and responsive when selections change.

The owner then requested checking that the rendering optimization does not significantly increase memory consumption and does not leak memory.
