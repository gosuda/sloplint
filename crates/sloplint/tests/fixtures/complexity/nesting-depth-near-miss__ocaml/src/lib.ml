let shallow (a : bool) (b : bool) (c : bool) : int =
  if a then
    if b then
      if c then 1 else 0
    else 0
  else 0
