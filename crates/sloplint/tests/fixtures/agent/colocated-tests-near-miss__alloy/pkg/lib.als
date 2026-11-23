module tour/lib

sig Counter {
  value: Int
}

pred fresh[c: Counter] {
  c.value = 0
}

pred bump[c, c': Counter] {
  c'.value = c.value
}
