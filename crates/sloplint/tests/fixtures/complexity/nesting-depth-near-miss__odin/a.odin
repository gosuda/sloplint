package probe

shallow :: proc(a: int) -> int {
    x := 0
    if a > 0 {
        if a > 1 {
            if a > 2 {
                x = 1
            }
        }
    }
    return x
}
