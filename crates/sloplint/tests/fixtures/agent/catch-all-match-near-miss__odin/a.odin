package probe

classify_number :: proc(n: int) -> string {
    switch n {
    case 1:
        return "one"
    case 2:
        return "two"
    case:
        return "other"
    }
}
