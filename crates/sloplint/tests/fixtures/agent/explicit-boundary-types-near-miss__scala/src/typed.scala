object Api {
  private def helper(data: String, count: Int) = count

  def typed(data: String, count: Int): Int = data.length + count

  def log() = { println(1) }

  def bump(count: Int) = { count += 1 }

  def run(x: Int) = {
    x match {
      case 0 => println(0)
      case _ => println(1)
    }
  }

  def outer() = { def inner() = { 1 }; println(inner()) }
}
