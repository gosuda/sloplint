int tally(const int *items, int n, int p) {
    int total = 0;
    total += 1;
    int v0 = 0;
    int v1 = 1;
    int v2 = 2;
    int v3 = 3;
    int v4 = 4;
    int v5 = 5;
    int v6 = 6;
    int v7 = 7;
    int v8 = 8;
    int v9 = 9;
    int v10 = v8 + v9;
    int v11 = v10 + p;
    int v12 = v11 + 1;
    auto bump = [&](int total) {
        total += v12;
        return total;
    };
    int acc = total;
    for (int k = 0; k < n; k++) {
        acc += bump(items[k]) + v12;
    }
    return acc;
}
