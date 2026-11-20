module tour/forward

sig S {}
sig Time {}
sig State {}

fun inner[x: S]: S {
  x
}

fun wrap[x: S]: S {
  some x
  inner[x]
}

// `run`/`check` targets are entry points, not passthroughs
pred runner[s: State] {
  inner_step[s]
}

pred inner_step[s: State] {
  some s
}

run runner
run this/runner

// primed declarators forward as primed args, not as their base names
pred step[s, s', t: State] {
  inner_pair[s, s']
}

pred inner_pair[a, b: State] {
  some a + b
}

run step for 4

// receiver calls forward `this` alongside non-param args
pred S.forward[this, x: S] {
  inner[this, S]
}

// trailing commas in decl lists and a let-expression member
pred misc[t: Time, ] {
  all q: S, | some q
  some s: S | s.let r = s | r = s
}

// a comment between call arguments is not an argument
fun argy[a, b: S]: S { a }
fun use_argy[x: S]: S {
  argy[x, /* pinned */ x]
}
