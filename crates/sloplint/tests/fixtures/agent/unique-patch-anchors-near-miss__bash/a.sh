step_first() {
  alpha_value=$(compute_first "$1")
  beta_value=$(compute_second "$alpha_value")
  gamma_value=$(compute_third "$beta_value")
  delta_value=$(compute_fourth "$gamma_value")
  epsilon_value=$(compute_fifth "$delta_value")
  store_result "$2" "$epsilon_value"
  finish_first
}

step_second() {
  alpha_value=$(compute_first "$1")
  beta_value=$(compute_second "$alpha_value")
  gamma_value=$(compute_third "$beta_value")
  delta_value=$(compute_fourth "$gamma_value")
  epsilon_value=$(compute_fifth "$delta_value")
  store_result "$2" "$epsilon_value"
  finish_second
}
