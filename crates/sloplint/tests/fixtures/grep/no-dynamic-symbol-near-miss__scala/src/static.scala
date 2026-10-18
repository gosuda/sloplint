object Static {
  def lookup(clazz: Class[_]): Unit = {
    clazz.getMethod("toString")
  }
}
