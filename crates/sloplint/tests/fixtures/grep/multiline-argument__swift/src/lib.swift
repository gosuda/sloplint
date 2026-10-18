public func compute(values: [Int]) -> Int {
    return values.count
}

public func callIt() -> Int {
    return compute(
        [
            1,
            2,
            3,
            4,
        ]
    )
}
