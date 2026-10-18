val limit = 10

fun capped(x: Int): Int {
    if (x > limit) {
        return limit
    }
    return x
}
