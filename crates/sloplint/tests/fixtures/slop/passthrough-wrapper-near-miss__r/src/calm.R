greet <- function(name, loud) {
  render_greeting(loud, name)
}

render_greeting <- function(name, loud) {
  if (loud) {
    toupper(name)
  } else {
    name
  }
}
