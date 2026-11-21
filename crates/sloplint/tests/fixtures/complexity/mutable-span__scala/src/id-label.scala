case class Doc(_id: Int)

object IdLabel {
  def score(d: Doc): Int = {
    var _id = 0
    _id = _id + 1
    val c01 = 1
    val c02 = 2
    val c03 = 3
    val c04 = 4
    val c05 = 5
    val c06 = 6
    val c07 = 7
    val c08 = 8
    val c09 = 9
    val c10 = 10
    val c11 = 11
    val c12 = 12
    val c13 = 13
    val c14 = 14
    val c15 = 15
    val c16 = 16
    val c17 = 17
    val c18 = 18
    val c19 = 19
    val c20 = 20
    val c21 = 21
    val c22 = 22
    val c23 = 23
    val c24 = 24
    val c25 = 25
    val c26 = 26
    val c27 = 27
    val c28 = 28
    val c29 = 29
    val c30 = 30
    d match {
      case Doc(_id = id) =>
        _id = _id + id + c30
        _id
      case _ => _id
    }
  }
}
