const int MaxCount = 8;

int Clamp(int v) {
    return v > MaxCount ? MaxCount : v;
}
