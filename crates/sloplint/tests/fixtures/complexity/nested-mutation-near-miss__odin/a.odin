package probe

shallow_sum :: proc(items: []int) -> int {
    total: int = 0
    for it in items {
        total += it
    }
    return total
}
