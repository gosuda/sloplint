module Probe {
  pure def shallow(a: bool, b: bool, c: bool, d: bool): int =
    if (a) if (b) if (c) if (d) 1 else 0 else 0 else 0 else 0
}
