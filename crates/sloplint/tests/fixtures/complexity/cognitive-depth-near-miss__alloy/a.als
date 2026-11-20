module tour/tangled

sig Book {}

pred tidy[a, b: Book] {
  all x: Book |
    all y: Book |
      a in x => b in y => some a else no b
}
