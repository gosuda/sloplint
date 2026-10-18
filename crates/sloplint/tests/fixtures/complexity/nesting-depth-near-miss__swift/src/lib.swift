public func shallow(a: Bool, b: Bool, c: Bool) -> Int {
    if a {
        if b {
            if c {
                return 1
            }
        }
    }
    return 0
}
