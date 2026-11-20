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
