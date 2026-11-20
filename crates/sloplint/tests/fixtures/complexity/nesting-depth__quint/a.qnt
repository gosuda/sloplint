module Probe {
  pure def deep(a: bool, b: bool, c: bool, d: bool, e: bool): int =
    if (a) if (b) if (c) if (d) if (e) 1 else 0 else 0 else 0 else 0 else 0
}
