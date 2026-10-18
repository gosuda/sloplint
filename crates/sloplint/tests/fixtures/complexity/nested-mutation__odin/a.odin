package probe

accumulate :: proc(items: []int) -> int {
    total: int = 0
    for it in items {
        for j in 0..<10 {
            for k in 0..<10 {
                total += it + j + k
            }
        }
    }
    return total
}
