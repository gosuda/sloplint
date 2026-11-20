module Probe {
  pure def convert(r: Rec): int = { a: r.a, b: r.b, c: r.c, d: r.d }.a
  type Rec = { a: int, b: int, c: int, d: int }
}
