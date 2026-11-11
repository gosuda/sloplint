int Deep(int a) {
    int r = 0;
    for (int i = 0; i < a; i++) {
        if (i > 0) {
            while (r < i) {
                switch (r) {
                default:
                    if (r > 5) {
                        r += 1;
                    }
                    break;
                }
                r += 1;
            }
        }
    }
    return r;
}
