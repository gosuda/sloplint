nested <- function(a, b, c) {
  for (i in 1:10) {
    if (a) {
      while (b) {
        for (j in 1:10) {
          if (c) {
            print(i)
          }
        }
      }
    }
  }
}
