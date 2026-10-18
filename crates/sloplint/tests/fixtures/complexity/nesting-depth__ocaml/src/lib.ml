let nested (a : bool) (b : bool) (c : bool) (d : bool) (e : bool) : int =
  if a then
    if b then
      if c then
        if d then
          if e then 1 else 0
        else 0
      else 0
    else 0
  else 0
