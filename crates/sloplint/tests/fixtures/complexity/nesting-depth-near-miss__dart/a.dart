int shallow(int a, int b, int c) {
  var out = 0;
  if (a > 0) {
    if (b > 0) {
      if (c > 0) {
        out = 1;
      }
    }
  }
  return out;
}
