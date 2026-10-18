package probe

deep :: proc(a: int) -> int {
    x := 0
    if a > 0 {
        if a > 1 {
            if a > 2 {
                if a > 3 {
                    if a > 4 {
                        x = 1
                    }
                }
            }
        }
    }
    return x
}
