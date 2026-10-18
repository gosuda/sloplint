int compute_first(int x) { return x + 1; }
int compute_second(int x) { return x + 2; }
int compute_third(int x) { return x + 3; }
int compute_fourth(int x) { return x + 4; }
int compute_fifth(int x) { return x + 5; }
void store_result(int *o, int v) { *o = v; }

void step_first(int input, int *output) {
    int alpha_value = compute_first(input);
    int beta_value = compute_second(alpha_value);
    int gamma_value = compute_third(beta_value);
    int delta_value = compute_fourth(gamma_value);
    int epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_first();
}

void step_second(int input, int *output) {
    int alpha_value = compute_first(input);
    int beta_value = compute_second(alpha_value);
    int gamma_value = compute_third(beta_value);
    int delta_value = compute_fourth(gamma_value);
    int epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_second();
}

void step_third(int input, int *output) {
    int alpha_value = compute_first(input);
    int beta_value = compute_second(alpha_value);
    int gamma_value = compute_third(beta_value);
    int delta_value = compute_fourth(gamma_value);
    int epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    finish_third();
}
