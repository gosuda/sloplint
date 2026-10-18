object Convert {
  def convert(p: Raw): Point = {
    Point(x = p.x, y = p.y, z = p.z, w = p.w)
  }
}
