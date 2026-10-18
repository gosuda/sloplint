object Partial {
  def convert3(p: Raw3): Point3 = {
    Point3(x = p.x, y = p.y, z = p.z)
  }

  def convert4(p: Raw3, extra: Long): Point4 = {
    Point4(x = p.x, y = p.y, z = p.z, w = extra)
  }
}
