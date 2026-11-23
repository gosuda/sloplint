---- MODULE RedColoc ----
EXTENDS Naturals
Init == x = 0
Next == x' = x + 1
Spec == Init /\ [][Next]_x
====
