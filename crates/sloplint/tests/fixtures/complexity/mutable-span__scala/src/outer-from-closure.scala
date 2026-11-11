object OuterFromClosure {
  def spread(items: List[Int], limit: Int): Int = {
    var total = 0
    total = total + 1
    val a01 = 1
    val a02 = 2
    val a03 = 3
    val a04 = 4
    val a05 = 5
    val a06 = 6
    val a07 = 7
    val a08 = 8
    val a09 = 9
    val a10 = 10
    val a11 = 11
    val a12 = 12
    val a13 = 13
    val a14 = 14
    val a15 = 15
    val a16 = 16
    val a17 = 17
    val a18 = 18
    val a19 = 19
    val a20 = 20
    val a21 = 21
    val a22 = 22
    val a23 = 23
    val a24 = 24
    val a25 = 25
    val a26 = 26
    val a27 = 27
    val tail = a27 + limit
    items.foreach { item =>
      total = total + item + tail
    }
    total
  }
}
