void risky();
void load() {
    try { risky(); } catch (...) {}
}
