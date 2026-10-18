class OrderId {
  final String value;
  OrderId(this.value);
}

void place(OrderId id, int qty) {
  print(qty);
}
