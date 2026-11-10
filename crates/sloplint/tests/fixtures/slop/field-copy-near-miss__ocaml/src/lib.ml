type triple = { a : int; b : int; c : int }

let convert (src : triple) : triple =
  { a = src.a; b = src.b; c = src.c }
