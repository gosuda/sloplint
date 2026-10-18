fun load(path: String): String {
    try {
        return readFile(path)
    } catch (e: Exception) {
    }
}

fun readFile(path: String): String {
    return path
}
