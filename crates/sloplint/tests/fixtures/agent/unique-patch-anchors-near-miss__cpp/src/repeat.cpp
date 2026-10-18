int compute_first(int x);
int compute_second(int x);
int compute_third(int x);
int compute_fourth(int x);
int compute_fifth(int x);
void store_result(int* o, int v);
int step_first(int input, int* output) {
    int alpha_value = compute_first(input);
    int beta_value = compute_second(alpha_value);
    int gamma_value = compute_third(beta_value);
    int delta_value = compute_fourth(gamma_value);
    int epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    return 1;
}
int step_second(int input, int* output) {
    int alpha_value = compute_first(input);
    int beta_value = compute_second(alpha_value);
    int gamma_value = compute_third(beta_value);
    int delta_value = compute_fourth(gamma_value);
    int epsilon_value = compute_fifth(delta_value);
    store_result(output, epsilon_value);
    return 2;
}
