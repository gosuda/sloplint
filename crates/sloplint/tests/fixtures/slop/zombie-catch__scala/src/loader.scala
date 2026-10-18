object Loader {
  def load(path: String): String = {
    try {
      readFile(path)
    } catch {
      case e: Exception => throw e
    }
  }
}
