object Reflect {
  def lookup(clazz: Class[_], name: String): Unit = {
    clazz.getMethod(name)
  }
}
