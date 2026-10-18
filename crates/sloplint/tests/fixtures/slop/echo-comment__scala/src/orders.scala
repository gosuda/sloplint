object Orders {
  /** Process the order. */
  def processOrder(id: Long, qty: Long): Long = id + qty
}
