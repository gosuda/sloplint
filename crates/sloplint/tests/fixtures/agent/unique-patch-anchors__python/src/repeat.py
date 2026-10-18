def step_first(data, output):
    alpha_value = compute_first(data)
    beta_value = compute_second(alpha_value)
    gamma_value = compute_third(beta_value)
    delta_value = compute_fourth(gamma_value)
    epsilon_value = compute_fifth(delta_value)
    store_result(output, epsilon_value)
    finish_first()

def step_second(data, output):
    alpha_value = compute_first(data)
    beta_value = compute_second(alpha_value)
    gamma_value = compute_third(beta_value)
    delta_value = compute_fourth(gamma_value)
    epsilon_value = compute_fifth(delta_value)
    store_result(output, epsilon_value)
    finish_second()

def step_third(data, output):
    alpha_value = compute_first(data)
    beta_value = compute_second(alpha_value)
    gamma_value = compute_third(beta_value)
    delta_value = compute_fourth(gamma_value)
    epsilon_value = compute_fifth(delta_value)
    store_result(output, epsilon_value)
    finish_third()
