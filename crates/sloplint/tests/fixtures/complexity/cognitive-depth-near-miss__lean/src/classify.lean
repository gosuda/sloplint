def classify (xs : Array Nat) : Nat := do
  let mut x := 0
  for i in xs do
    x := x + i
  pure x
