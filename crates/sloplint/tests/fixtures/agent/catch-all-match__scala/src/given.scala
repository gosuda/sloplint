object GivenCatchAll {
  sealed trait Shape
  case class Circle(r: Double) extends Shape

  def area(h: Any): Int = h match {
    case given(value) => 1
    case Circle(r) => 2
    case _ => 0
  }
}
