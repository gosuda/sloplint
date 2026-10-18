spread <- function(limit) {
  total <- 0
  a <- 1
  b <- 2
  c <- 3
  d <- 4
  e <- 5
  f <- 6
  g <- a + b
  h <- c + d
  i <- e + f
  j <- g + h
  k <- h + i
  m <- i + j
  n <- j + k
  o <- k + m
  p <- m + n
  q <- n + o
  r <- o + p
  s <- p + q
  t <- q + r
  u <- r + s
  v <- s + t
  w <- t + u
  x <- u + v
  y <- v + w
  z <- w + x
  aa <- y + z
  ab <- z + aa
  ac <- aa + ab
  ad <- ab + ac
  total <- total + z
  total <- total + limit
  total
}
