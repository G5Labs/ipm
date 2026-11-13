# Preview cancelled by idle stop — follow-up

## Human request

On 2026-10-06 the user asked why the preview for QA workspace `preview-test2`
(`qa.g5labs.ai/g5/preview-test2?view=activity`) showed "cancelled" at first.
After the diagnosis (the idle reaper stopped the VM 30 minutes after the preview
start while the preview build was still running; the activity view showed a bare
"cancelled"), the user said: "change it".

## Agent interpretation

Implement the two fixes proposed in the diagnosis:

1. A requested preview that has not yet reported ready counts as bounded work for
   idle selection, so a slow preview build does not consume the idle budget. A
   ready preview remains a service whose unused serving does not keep the VM up.
2. A preview ended by a workspace stop or loss records why in the preview task's
   outcome, and the activity view shows that reason instead of a bare "cancelled".

The default idle timeout and the rule that service output and keepalives are not
owner activity stay unchanged.
