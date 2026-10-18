object Deep {
  def nested(a: Boolean, b: Boolean, c: Boolean, d: Boolean, e: Boolean): Int = {
    if (a) {
      if (b) {
        if (c) {
          if (d) {
            if (e) {
              return 1
            }
          }
        }
      }
    }
    0
  }
}
