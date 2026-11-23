---- MODULE Depth ----
(*
--algorithm A
procedure First() begin
  return;
end procedure;
begin
  call First();
end algorithm;
\* sloplint-allow(complexity/nesting-depth): *)
Deep(a) ==
    IF a > 0 THEN
        IF a > 1 THEN
            IF a > 2 THEN
                IF a > 3 THEN
                    IF a > 4 THEN 1 ELSE 0
                ELSE 0
            ELSE 0
        ELSE 0
    ELSE 0
====
