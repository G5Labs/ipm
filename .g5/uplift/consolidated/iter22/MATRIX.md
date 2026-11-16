# Frozen neighboring outcomes

| Installed state / change | Reinstall result | Current supplier / version / code | History effect |
|---|---|---|---|
| Beta 1; older alpha offers 1 and 2 | requests 1, succeeds from alpha | alpha / 1 / payload 1 | one successful install, original reinstall command |
| Reset beta 1; disable alpha | requests 1, succeeds from beta | beta / 1 / payload 2 | one successful install |
| Delete both definitions; source directory still exists | default fails: no repositories | beta / 1 / payload 2 retained | one failed install row, visible summary |
| Same state, add dev flag | fails: no repositories | beta / 1 / payload 2 retained | one failed install row, visible summary |
| Missing installed-module name | early refusal | other installed module retained | no new install row |

Four successful snapshots, including initial/reset qualified beta, remain readable after deletion. The available version-2 source is a selection neighbor; a successful current-version-2 reinstall is not newly qualified. Snapshot flags are modeled only as source-derived configuration, without dependency payload claims. Failed detail-status rendering, failure row other fields, successful developer mode and general provider/error domains remain unassessed. These are builder-selected cases, not independent unseen discovery.
