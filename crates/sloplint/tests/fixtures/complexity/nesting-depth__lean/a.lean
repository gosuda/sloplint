def nested (a b c d e : Bool) : Nat :=
  if a then
    if b then
      if c then
        if d then
          if e then 1 else 0
        else 0
      else 0
    else 0
  else 0
