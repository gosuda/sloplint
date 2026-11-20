module Probe {
  var out: int
  pure val seed = 0
  action step_first = all {
    out' = compute_first(seed),
    out' = finish_first(out)
  }
  action step_second = all {
    out' = compute_first(seed),
    out' = finish_second(out)
  }
}
