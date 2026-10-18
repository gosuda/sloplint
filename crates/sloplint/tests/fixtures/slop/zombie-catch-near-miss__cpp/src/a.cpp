struct AppError {};
AppError wrap();
void risky();
void load() {
    try { risky(); } catch (const std::exception& e) { throw wrap(); }
}
