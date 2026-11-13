# Title bar removal mock

Sample-data prototype (variant B implemented 2026-10-11) for removing the 44px workspace title bar under the
alignment layout. It does not call G5.

Open `index.html` in a browser. The Variant toggle compares A (all former title-bar controls in the ribbon) with B (five global icons at the right end of the tab strip; owner, VM and workspace details stay in the ribbon; chosen 2026-10-11). The screen selector covers the workspace page
after the change, Home with workspace search and fork, the "+" fork picker with
search, and the current page for reference. The toolbar also switches the VM status (the Start/Stop button
relabels), and the viewer role (owner, org admin, member), which drives the
owner dropdown and VM controls.

`mock.fragment.html` is the editable prototype source; `index.html` is the same
page with a document skeleton. The notes under the frame list what moves
where and the assumptions to confirm before implementation.
