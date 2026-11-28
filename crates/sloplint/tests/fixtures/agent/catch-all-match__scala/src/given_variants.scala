object GivenVariants {
  sealed trait Shape
  case class Circle(r: Double) extends Shape
  case class Square(s: Double) extends Shape

  def area(h: Any): Int = h match {
    case given(Int, String) => 0
    case Circle(r) => 1
    case Square(s) => 2
    case _ => 3
  }

  def withNote(v: Any): Int = v match {
    case given(x, /* explanation */ y) => 1
    case Circle(r) => 2
    case _ => 0
  }

  def withParam[t](v: Any): Int = v match {
    case given(t, Int) => 1
    case Circle(r) => 2
    case _ => 0
  }
}
