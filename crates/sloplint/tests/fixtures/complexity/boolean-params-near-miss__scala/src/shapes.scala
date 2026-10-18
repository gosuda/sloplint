trait Renderer {
  def render(header: Boolean, footer: Boolean): String
}

class TableRenderer extends Renderer {
  override def render(header: Boolean, footer: Boolean): String = "t"
}

object Helpers {
  private def helper(flag: Boolean): String = "h"
}
