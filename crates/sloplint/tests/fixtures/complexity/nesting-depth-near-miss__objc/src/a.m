int Shallow(int a) {
    int r = 0;
    for (int i = 0; i < a; i++) {
        if (i > 0) {
            while (r < i) {
                r += 1;
            }
        }
    }
    return r;
}
