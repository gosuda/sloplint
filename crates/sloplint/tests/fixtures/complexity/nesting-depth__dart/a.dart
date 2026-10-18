int nested(int a, int b, int c, int d) {
  var out = 0;
  if (a > 0) {
    if (b > 0) {
      if (c > 0) {
        if (d > 0) {
          if (a > b) {
            out = 1;
          }
        }
      }
    }
  }
  return out;
}
