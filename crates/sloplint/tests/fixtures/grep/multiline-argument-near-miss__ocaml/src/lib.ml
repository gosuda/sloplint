let f (x : int) : int =
  compute (
    match x with
    | 0 -> one
    | _ -> other
  )
