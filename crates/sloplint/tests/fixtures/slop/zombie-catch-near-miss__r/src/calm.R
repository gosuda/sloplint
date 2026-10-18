load_data <- function(path) {
  tryCatch(
    read_data(path),
    error = function(e) {
      NULL
    }
  )
}
