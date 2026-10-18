fun loadChecked(path: String): String {
    try {
        return readFile(path)
    } catch (e: Exception) {
        throw IllegalStateException("cannot load", e)
    }
}

fun readFile(path: String): String {
    return path
}
