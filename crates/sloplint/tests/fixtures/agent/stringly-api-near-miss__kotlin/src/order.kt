data class OrderId(val value: String)

fun processWrapped(id: OrderId, qty: Long): String {
    return "${id.value} $qty"
}
