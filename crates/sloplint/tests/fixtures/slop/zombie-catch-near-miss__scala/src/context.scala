object Context {
  def load(path: String): String = {
    try {
      readFile(path)
    } catch {
      case e: Exception => throw new RuntimeException("ctx", e)
    }
  }
}
