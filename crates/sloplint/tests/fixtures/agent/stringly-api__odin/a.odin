package probe

configure_output :: proc(verbose: bool, colorize: bool, paginate: bool) -> string {
    if verbose {
        return "verbose"
    }
    if colorize {
        return "color"
    }
    if paginate {
        return "paged"
    }
    return "plain"
}
