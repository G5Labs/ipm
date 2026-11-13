------------------------------ MODULE Intent ------------------------------
VARIABLES committed, value
vars == <<committed, value>>
Init == /\ committed = FALSE /\ value = 0
Commit == /\ ~committed /\ committed' = TRUE /\ value' = 1
Next == Commit
Spec == Init /\ [][Next]_vars
=============================================================================
