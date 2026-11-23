module Probe {
  var state: int
  pure val idle = 0
  action init = all {
    state' = idle
  }
  action tickAction = all {
    state' = state
  }
  run first_run = init
    .then(tickAction)
    .then(all{
      assert(state == idle),
      state' = state
    })
    .then(tickAction)

  run second_run = init
    .then(tickAction)
    .then(all{
      assert(state == idle),
      state' = state
    })
    .then(tickAction)

  run third_run = init
    .then(tickAction)
    .then(all{
      assert(state == idle),
      state' = state
    })
    .then(tickAction)
}
