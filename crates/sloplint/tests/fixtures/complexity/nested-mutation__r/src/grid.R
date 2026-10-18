grid_sum <- function(n) {
  total <- 0
  for (i in 1:n) {
    for (j in 1:n) {
      for (k in 1:n) {
        total <<- total + i + j + k
      }
    }
  }
  total
}
