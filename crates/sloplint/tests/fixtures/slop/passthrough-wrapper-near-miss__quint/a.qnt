module Probe {
  pure def greet(name: str): str = render_greeting(name) + "!"
  pure def render_greeting(name: str): str = name
}
