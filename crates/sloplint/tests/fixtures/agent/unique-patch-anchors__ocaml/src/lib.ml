let step_first (input : int) (output : int) : int =
  let alpha_value = compute_first input in
  let beta_value = compute_second alpha_value in
  let gamma_value = compute_third beta_value in
  let delta_value = compute_fourth gamma_value in
  let epsilon_value = compute_fifth delta_value in
  store_result output epsilon_value
  |> finish_first

let step_second (input : int) (output : int) : int =
  let alpha_value = compute_first input in
  let beta_value = compute_second alpha_value in
  let gamma_value = compute_third beta_value in
  let delta_value = compute_fourth gamma_value in
  let epsilon_value = compute_fifth delta_value in
  store_result output epsilon_value
  |> finish_second

let step_third (input : int) (output : int) : int =
  let alpha_value = compute_first input in
  let beta_value = compute_second alpha_value in
  let gamma_value = compute_third beta_value in
  let delta_value = compute_fourth gamma_value in
  let epsilon_value = compute_fifth delta_value in
  store_result output epsilon_value
  |> finish_third

