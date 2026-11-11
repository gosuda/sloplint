let f (x : int) : int =
  compute (
    match x with
    | 0 -> one
    | 1 -> two
    | 2 -> three
    | _ -> other
  )
