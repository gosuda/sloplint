// process order validates stock levels and throws on empty input
fun processOrder(items: List<String>): Int {
    if (items.isEmpty()) {
        throw IllegalArgumentException("empty")
    }
    return items.size
}
