module Math {
  pure def pow(x: int, y: int): int = x
}
module Use {
  import Math.pow
  pure def cube(x: int): int = pow(x, 3)
}
