-------------------------- MODULE Implementation --------------------------
EXTENDS Naturals
CONSTANTS Mode, EarlyMapping
VARIABLES phase, stored, sawStage
vars == <<phase, stored, sawStage>>
Init == /\ phase = "idle" /\ stored = 0 /\ sawStage = FALSE
Stage == /\ phase = "idle" /\ Mode # "direct"
         /\ phase' = "staged" /\ stored' = stored /\ sawStage' = TRUE
Publish == /\ phase = "staged" /\ Mode # "stuck"
           /\ phase' = "done"
           /\ stored' = IF Mode = "bad_write" THEN 2 ELSE 1
           /\ UNCHANGED sawStage
Direct == /\ phase = "idle" /\ Mode = "direct"
          /\ phase' = "done" /\ stored' = 1 /\ UNCHANGED sawStage
Wait == UNCHANGED vars
Next == Stage \/ Publish \/ Direct \/ Wait
Spec == Init /\ [][Next]_vars

VisibleCommitted == IF EarlyMapping THEN phase # "idle" ELSE phase = "done"
Abstract == INSTANCE Intent WITH committed <- VisibleCommitted, value <- stored
RefinementStep == Abstract!Next
RefinesIntent == Abstract!Init /\ [][RefinementStep]_Abstract!vars

\* Negated reachability checks: a targeted violation supplies a finite witness.
NoStageThenCommit == ~(sawStage /\ phase = "done" /\ stored = 1)
NoDirectCommit == ~(~sawStage /\ phase = "done" /\ stored = 1)
WeakTypeOnly == /\ phase \in {"idle", "staged", "done"}
                /\ stored \in 0..2 /\ sawStage \in BOOLEAN
=============================================================================
