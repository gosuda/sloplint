module Probe {
  var out: int
  pure val seed = 0
  action step_first = all {
    out' = compute_first(seed),
    out' = compute_second(out),
    out' = compute_third(out),
    out' = compute_fourth(out),
    out' = compute_fifth(out),
    out' = store_result(out),
    out' = finish_first(out)
  }
  action step_second = all {
    out' = compute_first(seed),
    out' = compute_second(out),
    out' = compute_third(out),
    out' = compute_fourth(out),
    out' = compute_fifth(out),
    out' = store_result(out),
    out' = finish_second(out)
  }
  action step_third = all {
    out' = compute_first(seed),
    out' = compute_second(out),
    out' = compute_third(out),
    out' = compute_fourth(out),
    out' = compute_fifth(out),
    out' = store_result(out),
    out' = finish_third(out)
  }
}
