module tour/forward

sig S {}

fun inner[x: S]: S {
  x
}

fun wrap[x: S]: S {
  some x
  inner[x]
}
