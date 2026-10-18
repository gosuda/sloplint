object Main {
  val limit: Int = 100

  def main(args: Array[String]): Unit = {
    val home = sys.env("HOME")
    println(home)
  }
}
