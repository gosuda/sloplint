enum class Color {
    RED,
    GREEN,
    BLUE
}

fun describe(color: Color): String {
    return when (color) {
        Color.RED -> "red"
        Color.GREEN -> "green"
        else -> "other"
    }
}
