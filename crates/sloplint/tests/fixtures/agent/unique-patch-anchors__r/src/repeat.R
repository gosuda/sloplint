step_first <- function(input, output) {
  alpha_value <- compute_first(input)
  beta_value <- compute_second(alpha_value)
  gamma_value <- compute_third(beta_value)
  delta_value <- compute_fourth(gamma_value)
  epsilon_value <- compute_fifth(delta_value)
  store_result(output, epsilon_value)
  finish_first()
}

step_second <- function(input, output) {
  alpha_value <- compute_first(input)
  beta_value <- compute_second(alpha_value)
  gamma_value <- compute_third(beta_value)
  delta_value <- compute_fourth(gamma_value)
  epsilon_value <- compute_fifth(delta_value)
  store_result(output, epsilon_value)
  finish_second()
}

step_third <- function(input, output) {
  alpha_value <- compute_first(input)
  beta_value <- compute_second(alpha_value)
  gamma_value <- compute_third(beta_value)
  delta_value <- compute_fourth(gamma_value)
  epsilon_value <- compute_fifth(delta_value)
  store_result(output, epsilon_value)
  finish_third()
}
