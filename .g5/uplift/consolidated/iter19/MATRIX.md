# Retained physical duplicate origin

| Fixture | Constructor order | Cache/service retained | Actual installed reader |
|---|---|---|---|
| single |00-a:A|00-a/A,1row,0warnings|Description source-A,root00-a,repo single,code101|
| ab |00-a:A then99-b:B|00-a/A,1row,1warning+OK|Description source-A,root00-a,repo ab,code101|
| ba |00-b:B then99-a:A|00-b/B,1row,1warning+OK|Description source-B,root00-b,repo ba,code202|

All offer iter19-dupe1.0.0 with the same class resource but different Description/code. Cache manifest and service manifest are byte-equal in the native fixture; both API directories and installed roots agree. Show prints the expected description/root/supplier, list includes the module. Constructor output and independent sampled ScanDirectory agree; this is not a portable lexical or cross-call guarantee. Cache row IDs1/2/4 are observations, not a modeled allocator promise.
