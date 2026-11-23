---- MODULE Depth ----
(*
--algorithm A
\* sloplint-allow(complexity/nesting-depth): deliberate nesting
procedure First(a) begin
  while a > 0 do
    while a > 1 do
      while a > 2 do
        while a > 3 do
          while a > 4 do
            a := a - 1;
          end while;
        end while;
      end while;
    end while;
  end while;
  return;
end procedure;
procedure Second(b) begin
  while b > 0 do
    while b > 1 do
      while b > 2 do
        while b > 3 do
          while b > 4 do
            b := b - 1;
          end while;
        end while;
      end while;
    end while;
  end while;
  return;
end procedure;
begin
  call First(1);
  call Second(2);
end algorithm;
*)
Spec == TRUE
============================================================================
