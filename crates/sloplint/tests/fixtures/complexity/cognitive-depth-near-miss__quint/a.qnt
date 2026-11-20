module Probe {
  var x: int
  pure val bound = 5
  action calm = all {
    if (bound > 0) x' = 1 else x' = 0,
    if (bound > 1) x' = 2 else x' = 0
  }
}
