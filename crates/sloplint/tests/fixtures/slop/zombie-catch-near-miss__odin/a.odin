package probe

load_value :: proc() -> (int, bool) {
    return 1, true
}

fetch_data :: proc() -> int {
    v, ok := load_value()
    if !ok {
        return -1
    }
    return v
}
