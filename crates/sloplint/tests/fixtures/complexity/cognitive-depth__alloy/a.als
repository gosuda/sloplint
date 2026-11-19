module tour/tangled

sig Book {}

pred tangled[a, b: Book] {
  all x: Book |
    all y: Book |
      all z: Book |
        a in x => b in y => x in a => some z else no b
}
