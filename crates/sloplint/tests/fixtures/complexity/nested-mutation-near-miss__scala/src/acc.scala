object Acc {
  def acc(n: Int): Int = {
    var total = 0
    for (i <- 0 until n) {
      total = total + i
    }
    total
  }
}
