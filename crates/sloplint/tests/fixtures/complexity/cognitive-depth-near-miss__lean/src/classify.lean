def classify (xs ys zs : Array Nat) : IO Unit := do
  for i in xs do
    for j in ys do
      for k in zs do
        IO.println (i + j + k)
