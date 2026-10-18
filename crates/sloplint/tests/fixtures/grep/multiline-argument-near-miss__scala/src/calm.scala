object Calm {
  def f(): Unit = {
    run(
      (x: Int) => {
        val y = x + 1
        y * 2
      }
    )
  }
}
