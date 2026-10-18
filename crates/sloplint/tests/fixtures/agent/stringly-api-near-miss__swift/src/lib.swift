public struct OrderId {
    public var raw: Int
    public init(raw: Int) {
        self.raw = raw
    }
}

public func settle(id: OrderId, qty: Int) -> Int {
    return qty
}
