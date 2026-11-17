# Improve an existing Verus uplift — builder prompt

Use the shared [uplift iteration prompt](../uplift-iteration.md) with the [Verus uplift procedure](uplift.md), [verification rules](verification.md) and [playbook](verification-playbook.txt). Its scope selection, source discovery, scoring, handoff and convergence rules are shared with TLC; the Verus evidence requirements remain specific to this backend.

For a requested continuous run, follow the [master loop](../uplift.md#when-an-iterative-loop-is-requested). A worker completes its assigned batch and returns evidence and residual work; the master owns the next assignment and the stopping decision.
