# Iteration 12 observations

| Neighbor | Observed result |
|---|---|
| Preview / repos without map | Preview succeeds without writes; invalid flags fail without writes |
| First map+repos into empty target | Code mapped; repository D/S/I absent |
| Existing target-local repository | Subsequent map skips sharing; own name remains visible |
| Clear target-local repository, retry | Source D/S/I mapped; source name becomes visible |
| Repos-only unmap | Code retained, repository mapping removed |
| Full unmap after remap | Package/global removed, routine retained; fresh version command still succeeds |
| Local vs mapped IPMVersion load | Source passes and installs; target without local zpm row fails and retains no new row |
| K/L filesystem lockfiles | Shared dependencies once, exact installed version/repository/requirements |
| Reverse-name graph | Flat order zz-leaf,aa-parent; JSON aa-parent,zz-leaf; inherited P:F-DEP-2 |
| Cursor challenge | bb,cc,dd,aa; destructive graph exhausted |
| Duplicate/forged direct reference | One entry using installed record by Name |
| Repository projection | Named credential fields absent; allowed URL retained |
| Invalid later namespace / activation failure | Unassessed; strongest next candidate |
