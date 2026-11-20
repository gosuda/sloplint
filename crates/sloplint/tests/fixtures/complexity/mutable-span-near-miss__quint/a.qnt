module Probe {
  var total: int
  action bump = all {
    total' = 1,
    total' = total + 1
  }
}
