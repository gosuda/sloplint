pub fn step_first(input: u64, output: *u64) void {
    const alpha_value = compute_first(input);
    const beta_value = compute_second(alpha_value);
    const gamma_value = compute_third(beta_value);
    const delta_value = compute_fourth(gamma_value);
    const epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_first();
}

pub fn step_second(input: u64, output: *u64) void {
    const alpha_value = compute_first(input);
    const beta_value = compute_second(alpha_value);
    const gamma_value = compute_third(beta_value);
    const delta_value = compute_fourth(gamma_value);
    const epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_second();
}

pub fn step_third(input: u64, output: *u64) void {
    const alpha_value = compute_first(input);
    const beta_value = compute_second(alpha_value);
    const gamma_value = compute_third(beta_value);
    const delta_value = compute_fourth(gamma_value);
    const epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_third();
}
