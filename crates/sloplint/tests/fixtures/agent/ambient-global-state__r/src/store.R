home_dir <- Sys.getenv("HOME")

configure <- function() {
  Sys.setenv(APP_MODE = "prod")
}
