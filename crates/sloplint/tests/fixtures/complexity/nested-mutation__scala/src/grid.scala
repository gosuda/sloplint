object Grid {
  def gridSum(n: Int): Int = {
    var total = 0
    for (i <- 0 until n) {
      for (j <- 0 until n) {
        for (k <- 0 until n) {
          total = total + i + j + k
        }
      }
    }
    total
  }
}
