module Probe {
  pure def greet(name: str, loud: bool): str = render_greeting(name, loud)
  pure def render_greeting(name: str, loud: bool): str = name
}
