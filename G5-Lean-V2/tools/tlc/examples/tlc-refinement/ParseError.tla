------------------------------ MODULE ParseError ------------------------------
\* Deliberately invalid: proves a parser error is not an expected counterexample.
VARIABLE x
Init == x =
Next == x' = x
Spec == Init /\ [][Next]_x
Safe == x = 0
=============================================================================
