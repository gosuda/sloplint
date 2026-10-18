pub fn step_first(input: u64, output: &mut u64) {
    let alpha_value = compute_first(input);
    let beta_value = compute_second(alpha_value);
    let gamma_value = compute_third(beta_value);
    let delta_value = compute_fourth(gamma_value);
    let epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_first();
}

pub fn step_second(input: u64, output: &mut u64) {
    let alpha_value = compute_first(input);
    let beta_value = compute_second(alpha_value);
    let gamma_value = compute_third(beta_value);
    let delta_value = compute_fourth(gamma_value);
    let epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_second();
}
