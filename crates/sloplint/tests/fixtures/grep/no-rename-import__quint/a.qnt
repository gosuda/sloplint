module Math {
  pure val value = 1
}
module Use {
  import Math as Arithmetic
  pure val x = Arithmetic.value
}
