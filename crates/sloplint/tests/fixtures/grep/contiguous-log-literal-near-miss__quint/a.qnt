module Probe {
  pure def f = g("hello")
  pure def g(x: str): str = x
}
