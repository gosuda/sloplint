package probe

calculate_total :: proc(items: []int) -> int {
    total := 0
    for it in items {
        total += it
    }
    return total
}

@(test)
check_calculate_total :: proc() {
    assert(calculate_total([]) == 0, "empty input gives zero")
}
