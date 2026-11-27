int settle(int limit) {
    int first = 0, second = 0;
    first += 1;
    second += 1;
    int pad_a = first + 1;
    int pad_b = pad_a + 2;
    int pad_c = pad_b + 3;
    int pad_d = pad_c + 4;
    int pad_e = pad_d + 5;
    int pad_f = pad_e + 6;
    int pad_g = pad_f + 7;
    int pad_h = pad_g + 8;
    int pad_i = pad_h + 9;
    int pad_j = pad_i + 10;
    int pad_k = pad_j + 11;
    int pad_l = pad_k + 12;
    int pad_m = pad_l + 13;
    first += 2;
    return first + second + pad_m + limit;
}
