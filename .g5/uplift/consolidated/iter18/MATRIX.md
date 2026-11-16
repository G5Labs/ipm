# Ordinary same-version supplier changes

| Call | Current module/code | Successful history and actual reader |
|---|---|---|
| `install alpha/iter16-origin 1.0.0` | alpha/rootA/payload1, version1.0.0 | New alpha source, actual permanent2 |
| `install beta/iter16-origin 1.0.0` without uninstall | beta/rootB/payload2, same version | New beta source/permanent3; old alpha retained |
| `install missing/iter16-origin 1.0.0` | Error, beta/rootB/payload2 retained | Earlier successful sources retained; rejected record shape unassessed |
| `install alpha/iter16-origin 1.0.0` again | alpha/rootA/payload1, same version | New alpha source/permanent5; old alpha/beta retained |

Module/class/root literals are reused from16 in a new private container, while fixture sources and receipts live under18. The current supplier appears in show and only its repository-filtered list. History details preserve the source endpoint for each past action. Both directions of actual equal-version Follows return0. Raw IDs are observations; logical successful labels0/1/2 do not model the database counter or failed-history record.
