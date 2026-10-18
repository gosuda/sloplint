class OrderId(val value: Long)

object Api {
  def processOrder(id: OrderId, qty: Long): Long = qty
}
