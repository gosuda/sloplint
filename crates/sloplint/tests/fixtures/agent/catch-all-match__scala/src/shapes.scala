object Shapes {
  sealed trait Shape
  case class Circle(r: Double) extends Shape
  case class Square(s: Double) extends Shape

  def area(h: Shape): Double = h match {
    case Circle(r) => r * r * 3.14
    case _ => 0.0
  }
}
