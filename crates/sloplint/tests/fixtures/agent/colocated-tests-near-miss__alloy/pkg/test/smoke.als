module tour/smoke

sig Probe {}

pred healthy[p: Probe] {
  some p
}

run healthy
