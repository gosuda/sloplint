module NestProbe {
  var x: int
  pure def classify(n: int): int =
    if (n == 0) 0
    else if (n == 1) 1
    else if (n == 2) 2
    else if (n == 3) 3
    else if (n == 4) 4
    else if (n == 5) 5
    else 6
  action init = all {
    x' = classify(x)
  }
}
