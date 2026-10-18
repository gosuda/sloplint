object Greet {
  def greet(name: String, loud: Boolean): String = {
    renderGreeting(name, loud)
  }

  def renderGreeting(name: String, loud: Boolean): String = name
}
