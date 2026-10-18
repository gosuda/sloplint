# Process the order.
#
# Errors when the quantity is negative.
process_order <- function(id, qty) {
  if (qty < 0) {
    stop("negative quantity")
  }
  id + qty
}
