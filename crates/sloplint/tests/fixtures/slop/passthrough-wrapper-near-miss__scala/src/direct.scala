object Direct {
  def moveTo(x: Int, y: Int): Int = place(y, x)

  def place(x: Int, y: Int): Int = x + y

  def fresh(): List[Int] = List.empty[Int]
}
