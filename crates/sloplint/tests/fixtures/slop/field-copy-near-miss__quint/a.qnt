module Probe {
  pure def convert(r: Rec, s: Rec): int = { a: r.a, b: s.b, c: r.c, d: r.d }.a
  type Rec = { a: int, b: int, c: int, d: int }
}
