module tour/nesting

sig Book {}

pred shallow[a, b: Book] {
  all x: Book |
    all y: Book |
      a in x => b in y => some a else no b
}
