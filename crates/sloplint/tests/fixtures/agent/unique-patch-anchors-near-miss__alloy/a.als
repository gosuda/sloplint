module tour/anchors

sig State {
  moved: lone State,
  holder: lone State,
  turn: lone State,
  phase: lone State,
  flags: lone State,
  order: lone State
}

pred stepA[s: State] {
  s.moved = s.moved
  s.holder = s.holder
  s.turn = s.turn
  s.phase = s.phase
  s.flags = s.flags
  s.order = s.order
}

pred stepB[s: State] {
  s.moved = s.moved
  s.holder = s.holder
  s.turn = s.turn
  s.phase = s.phase
  s.flags = s.flags
  s.order = s.order
}

pred stepC[s: State] {
  s.moved = s.moved
  s.holder = s.holder
  s.turn = s.turn
  s.phase = s.phase
  s.flags = s.flags
  s.order = s.order
}
