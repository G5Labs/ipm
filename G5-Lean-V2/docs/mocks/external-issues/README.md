# External issue integration mocks

These sample-data prototypes accompany [the design](../../external-issue-integration-design.md). They do not call G5, GitHub, or Jira.

Open `index.html` in a browser. The screen selector covers Home, fork, linked issues, chronological Activity, issue backlinks, and tracker unavailability. The fork screen also demonstrates optional sub-issue creation. Use the source workspace and issue selectors to explore it.

`mock.fragment.html` is the editable prototype source; `index.html` is a standalone export with its presentation wrapper. The PNGs are captured reference states used in the Markdown design.

Verified prototype interactions include searching and filtering, primary-ownership conflict handling, sub-issue fork preview, a shared comment appearing in both G5 and Jira previews, and a tracker outage that disables posting while allowing a draft. All six screens were checked for horizontal overflow at a 320-pixel content width, with desktop captures at 1024 pixels. These checks validate the prototype only.

Provider-specific required-field pickers, advanced fork settings, source-record drilldowns, and actual ownership transfer are described in the design and simplified in the prototype. The tracker-unavailable view is a sample state, not a network simulation. Earlier exploratory mocks from the conversation are superseded by this set.
