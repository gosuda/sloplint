module tour/catalog

sig Book {}

// the add
pred add[b: Book] {
  some b
}
