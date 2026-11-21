def tangled (a : Nat) (xs ys : Array Nat) : IO Unit := do
  for i in xs do
    for j in ys do
      IO.println (if a > 0 then i + j else 0)
      IO.println (if a > 1 then i + j else 0)
      IO.println (if a > 2 then i + j else 0)
      IO.println (if a > 3 then i + j else 0)
      IO.println (if a > 4 then i + j else 0)
      IO.println (if a > 5 then i + j else 0)
