# Owner scope correction

On 2026-11-04, after objecting to the failed-draft tray, new status labels and
implementation size, the owner instructed:

> i only need you to fix the frontend, and let me know if the fix will have to introduce more failures and corner case that need to be addressed, revert out-of-scope fix

Agent interpretation: keep the existing queue and composer interactions; revert
backend/API changes and the new recovery workflow. Report limitations of the
existing contract rather than expanding this PR to resolve them.
