trait Draw {
  def draw(): Unit
}

trait Orphan {
  def run(): Unit
}

trait WidgetExt {
  def describe(): String
}
