module Probe {
  var counter: int
  action bump = all {
    counter' = counter + 1
  }
}
