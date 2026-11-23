module Probe {
  var total: int
  action bump = all {
    total' = 1,
    total' = total + 1
  }
  action step = any {
    { nondet v = 1
      total' = v },
    { nondet v = 2
      total' = v },
    { nondet v = 3
      total' = v }
  }
}
