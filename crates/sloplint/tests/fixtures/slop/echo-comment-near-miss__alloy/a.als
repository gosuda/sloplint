module tour/catalog

sig Book {}

// add the book to the catalog
pred add[b: Book] {
  some b
}
