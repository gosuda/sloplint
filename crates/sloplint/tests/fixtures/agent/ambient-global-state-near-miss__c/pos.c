static const int LIMIT = 1024;

int capped(int n) {
    if (n > LIMIT) {
        return LIMIT;
    }
    return n;
}
