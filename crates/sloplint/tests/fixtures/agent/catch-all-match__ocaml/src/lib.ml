let describe (c : int option) : string =
  match c with
  | Some x -> "some"
  | None -> "none"
  | _ -> "other"
