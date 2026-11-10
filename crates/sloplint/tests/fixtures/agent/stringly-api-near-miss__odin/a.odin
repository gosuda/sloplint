package probe

configure_output :: proc(verbose: bool, colorize: bool, format: string) -> string {
    if verbose {
        return "verbose"
    }
    if colorize {
        return "color"
    }
    return format
}
