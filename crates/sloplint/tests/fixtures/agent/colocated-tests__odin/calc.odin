package probe

calculate_total :: proc(items: []int) -> int {
    total := 0
    for it in items {
        total += it
    }
    return total
}
