let counter = ref 0

let bump () =
  counter := !counter + 1

let load () =
  counter := String.length (Sys.getenv "HOME")
