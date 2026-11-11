let load (key : string) : string =
  try read_key key with
  | e -> raise e

let read_key (key : string) : string = key
