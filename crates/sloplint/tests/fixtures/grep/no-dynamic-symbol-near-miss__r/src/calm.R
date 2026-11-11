lookup_value <- function() {
  get("fixed_name")
}

passthrough <- function(FUN, x) {
  FUN <- match.fun(FUN)
  FUN(x)
}
