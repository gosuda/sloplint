public func nested(a: Bool, b: Bool, c: Bool, d: Bool, e: Bool) -> Int {
    if a {
        if b {
            if c {
                if d {
                    if e {
                        return 1
                    }
                }
            }
        }
    }
    return 0
}
