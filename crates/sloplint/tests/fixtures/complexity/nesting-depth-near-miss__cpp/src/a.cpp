int shallow(bool a, bool b, bool c) {
    if (a) {
        if (b) {
            if (c) { return 1; }
        }
    }
    return 0;
}
