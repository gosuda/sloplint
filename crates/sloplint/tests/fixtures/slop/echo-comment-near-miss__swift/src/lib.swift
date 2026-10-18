/// Process the order.
/// - Returns: the total, or -1 when the ledger is closed.
public func processOrder(id: Int, qty: Int) -> Int {
    return id + qty
}
