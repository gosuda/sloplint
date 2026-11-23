module Probe {
  var balance: int
  var state: int
  action holdA = all {
    balance' = balance,
    state' = state
  }
  action holdB = all {
    balance' = balance,
    state' = state
  }
  action holdC = all {
    balance' = balance,
    state' = state
  }
}
