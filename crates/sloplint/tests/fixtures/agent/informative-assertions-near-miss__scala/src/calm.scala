object Calm {
  def checkReady(ready: Boolean): Unit = {
    assert(ready, "service must be ready")
  }
}
