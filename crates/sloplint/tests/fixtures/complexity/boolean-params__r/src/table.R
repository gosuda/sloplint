render_table <- function(header = TRUE, footer = TRUE) {
  out <- ""
  if (header) {
    out <- "h"
  }
  if (footer) {
    out <- paste0(out, "f")
  }
  out
}
