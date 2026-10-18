public struct OrderId {
    public var raw: Int
    public init(raw: Int) {
        self.raw = raw
    }
}

public func processOrder(id: String, account: String, qty: Int) -> Int {
    return qty
}
