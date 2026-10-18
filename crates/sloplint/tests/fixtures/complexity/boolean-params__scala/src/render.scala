object Render {
  def renderTable(header: Boolean, footer: Boolean): String = {
    val h = if (header) "h" else ""
    val f = if (footer) "f" else ""
    h + f
  }
}
