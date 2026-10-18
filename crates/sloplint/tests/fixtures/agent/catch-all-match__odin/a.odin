package probe

Color :: enum { Red, Green, Blue }

classify :: proc(c: Color) -> string {
    switch c {
    case .Red:
        return "red"
    case .Green:
        return "green"
    case:
        return "other"
    }
}
