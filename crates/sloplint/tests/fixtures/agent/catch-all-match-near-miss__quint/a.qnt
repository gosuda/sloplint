module Probe {
  type T = A(int) | B
  pure def f(v: T): int = match v { A(x) => x | B => 0 }
}
