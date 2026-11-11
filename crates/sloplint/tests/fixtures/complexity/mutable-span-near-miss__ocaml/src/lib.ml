let spread (limit : int) : int =
  let total = ref 0 in
  total := !total + 1;
  total := !total + limit;
  !total
