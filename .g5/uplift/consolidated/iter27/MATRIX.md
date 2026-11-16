# Uninstall purge and final data reader

| Module | Command option | Callback argument | Post-reload rows | Sentinel value |
|---|---|---:|---:|---:|
| no installer | none | absent | 1 | 271 |
| no installer | -purge | absent | 1 | 271 |
| no installer | -DConfigure.Purge=1 | absent | 1 | 271 |
| fixture installer | none | 0 | 1 | 271 |
| fixture installer | -purge | 0 | 1 | 271 |
| fixture installer | -DConfigure.Purge=1 | 1 | 0 | 0 (empty aggregate) |

Every uninstall removes the checked class and module record and succeeds. Reloading the same directory/schema makes retained data readable; class absence alone says nothing about row deletion. The fixture callback explicitly calls `%DeleteExtent` only when its actual purge argument is true. Independent direct SDK calibration also removes the sentinel while leaving the class usable. SQL statuses, reset/seed, load/uninstall/reload, callback entry/exit and code markers are separate gates. Token is the logical string `sentinel`; no opaque IRIS ID is equated to a modeled key.
