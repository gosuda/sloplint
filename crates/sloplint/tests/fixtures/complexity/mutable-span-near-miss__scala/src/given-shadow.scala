object GivenShadow {
  def tally(v: Any): Int = {
    var _status = 0
    _status = _status + 1
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
    v match {
      case given(_status, _) =>
        _status += 1
        _status
      case _ => 0
    }
  }
}
