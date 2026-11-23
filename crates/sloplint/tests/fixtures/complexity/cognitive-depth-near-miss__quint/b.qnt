module Probe {
  var x: int
  pure val bound = 5
  action tangled = all {
    if (bound > 0) x' = 1 else x' = 0,
    if (bound > 1) x' = 2 else x' = 0,
    if (bound > 2) x' = 3 else x' = 0,
    if (bound > 3) x' = 4 else x' = 0,
    if (bound > 4) x' = 5 else x' = 0,
    if (bound > 5) x' = 6 else x' = 0,
    if (bound > 6) x' = 7 else x' = 0,
    if (bound > 7) x' = 8 else x' = 0,
    if (bound > 8) x' = 9 else x' = 0,
    if (bound > 9) x' = 10 else x' = 0,
    if (bound > 10) x' = 11 else x' = 0,
    if (bound > 11) x' = 12 else x' = 0
  }
}
