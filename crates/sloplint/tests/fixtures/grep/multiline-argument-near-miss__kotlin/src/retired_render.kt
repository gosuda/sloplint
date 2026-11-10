fun render(): String {
    return format(
        "title",
        combine(
            "a",
            "b",
            "c",
            "d"
        )
    )
}

fun format(title: String, body: String): String {
    return title + body
}

fun combine(a: String, b: String): String {
    return a + b
}
