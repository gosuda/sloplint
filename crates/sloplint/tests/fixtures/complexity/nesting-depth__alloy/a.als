module tour/nesting

sig Book {}

pred deep[a, b: Book] {
  all x: Book |
    all y: Book |
      a in x => b in y => x in a => some b else no a
}
