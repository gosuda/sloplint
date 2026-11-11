counter <- 0

bump <- function() {
  assign("counter", counter + 1, envir = .GlobalEnv)
}

configure <- function() {
  Sys.setenv(APP_MODE = "prod")
}

total <- 0

reset <- function() {
  total <<- 0
}
