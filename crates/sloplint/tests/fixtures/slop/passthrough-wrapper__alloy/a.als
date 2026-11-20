module tour/forward

sig S {}

fun inner[x: S]: S {
  x
}

fun wrap[x: S]: S {
  inner[x]
}
