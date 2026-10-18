public class Orders {
    public ulong Place(OrderId id, int qty) {
        return (ulong)qty;
    }
}

public class OrderId {
    public ulong Value { get; }
}
