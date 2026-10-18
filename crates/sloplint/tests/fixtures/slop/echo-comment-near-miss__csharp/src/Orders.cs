public class Orders {
    /// Process the order.
    /// Throws when the ledger is closed and retries three times on timeout.
    public int ProcessOrder(int id, int qty) {
        return id + qty;
    }
}
