public func gridSum(n: Int) -> Int {
    var total = 0
    for i in 0..<n {
        for j in 0..<n {
            for k in 0..<n {
                total += i + j + k
            }
        }
    }
    return total
}
