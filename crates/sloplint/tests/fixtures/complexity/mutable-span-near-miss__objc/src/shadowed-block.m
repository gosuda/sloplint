int Tally(int *items, int n, int p) {
    int total = n;
    total += 1;
    int pad01 = 1;
    int pad02 = 2;
    int pad03 = 3;
    int pad04 = 4;
    int pad05 = 5;
    int pad06 = 6;
    int pad07 = 7;
    int pad08 = 8;
    int pad09 = 9;
    int pad10 = 10;
    int pad11 = 11;
    int pad12 = 12;
    int pad13 = 13;
    int pad14 = 14;
    int pad15 = 15;
    int pad16 = 16;
    int pad17 = 17;
    int pad18 = 18;
    int pad19 = 19;
    void (^bump)(int) = ^(int total) {
        total += p;
    };
    for (int k = 0; k < n; k++) {
        bump(items[k]);
    }
    return total;
}
