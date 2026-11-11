let load (key : string) : string =
  try read_key key with
  | Not_found -> prerr_endline "read failed"
  | e -> raise e

let read_key (key : string) : string = key
