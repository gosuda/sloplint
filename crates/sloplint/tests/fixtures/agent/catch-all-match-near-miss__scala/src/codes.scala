object Codes {
  def classify(code: Int): String = code match {
    case 0 => "zero"
    case 1 => "one"
    case _ => "many"
  }
}
