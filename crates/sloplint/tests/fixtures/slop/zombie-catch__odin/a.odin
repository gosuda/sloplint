package probe

load_value :: proc() -> (int, bool) {
    return 1, true
}

fetch_data :: proc() -> int {
    v := load_value() or_return
    return v
}
