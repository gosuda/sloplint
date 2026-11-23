module tour/forward

sig S {}

fun inner[x: S]: S {
  x
}

fun wrap[x: S]: S {
  inner[x]
}

pred pair[a, b: S] {
  a = b
}

pred tick[s, s': S] {
  pair[s, s']
}

// a sole infix helper call is still a passthrough wrapper
fun infix_wrap[a, b: S]: S {
  a fun/add b
}

// `run step'` targets `step'`, not `step`: the unprimed
// wrapper still flags while its primed sibling is suppressed
pred step'[x: S] {
  inner[x]
}
run step'
pred step[x: S] {
  inner[x]
}
