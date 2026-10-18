object ServiceSpec {
  def checkRender(): Unit = {
    assert(Service.render("x") == "x", "renders names")
  }
}
