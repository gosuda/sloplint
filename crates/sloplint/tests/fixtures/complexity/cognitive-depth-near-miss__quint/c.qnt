module Probe {
  var phase: int
  action step = if (phase == 0) phase' = 1
    else if (phase == 1) phase' = 2
    else if (phase == 2) phase' = 3
    else if (phase == 3) phase' = 4
    else if (phase == 4) phase' = 5
    else if (phase == 5) phase' = 6
    else phase' = 0
}
