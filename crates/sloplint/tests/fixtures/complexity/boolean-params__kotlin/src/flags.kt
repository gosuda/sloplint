fun setFlags(fast: Boolean, safe: Boolean): String {
    if (fast) {
        return "fast"
    }
    if (safe) {
        return "safe"
    }
    return "none"
}
