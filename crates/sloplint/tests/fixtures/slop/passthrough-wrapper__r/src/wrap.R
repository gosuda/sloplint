greet <- function(name, loud) {
  render_greeting(name, loud)
}

render_greeting <- function(name, loud) {
  if (loud) {
    toupper(name)
  } else {
    name
  }
}
