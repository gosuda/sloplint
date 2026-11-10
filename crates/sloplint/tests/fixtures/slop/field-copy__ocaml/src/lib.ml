type point = { a : int; b : int; c : int; d : int }

let convert (src : point) : point =
  { a = src.a; b = src.b; c = src.c; d = src.d }
