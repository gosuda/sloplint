module Probe {
  type T = A(int) | B | C
  pure def f(v: T): int = match v { A(x) => x | B => 0 | _ => 0 }
}
