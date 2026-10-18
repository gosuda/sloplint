public func classify(code: Int) -> String {
    switch code {
    case 0:
        return "zero"
    case 1:
        return "one"
    default:
        return "many"
    }
}
