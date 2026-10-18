object Deep {
  def shallow(a: Boolean, b: Boolean, c: Boolean): Int = {
    if (a) {
      if (b) {
        if (c) {
          return 1
        }
      }
    }
    0
  }
}
