object Calm {
  def load(path: String, reason: String): Unit = {
    println(s"failed to open $path: $reason")
  }
}
