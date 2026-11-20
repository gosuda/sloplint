def shallow (a b c : Bool) : Nat :=
  if a then
    if b then
      if c then 1 else 0
    else 0
  else 0
