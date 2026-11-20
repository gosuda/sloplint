module tour/naming

sig Book {}

pred helper[a: Book] {
  some a
}

pred lookup[b: Book] {
  some b
}
