let main () : unit =
  let home = Sys.getenv "HOME" in
  print_endline home
