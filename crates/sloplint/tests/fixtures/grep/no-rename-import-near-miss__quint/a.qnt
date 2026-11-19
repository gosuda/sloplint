module Math {
  pure val value = 1
}
module Use {
  import Math
  pure val x = Math.value
}
