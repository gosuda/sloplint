int shallow(int a, int b) {
    for (int i = 0; i < a; i++) {
        if (a > i) {
            while (b > 0) {
                b--;
            }
        }
    }
    return b;
}
