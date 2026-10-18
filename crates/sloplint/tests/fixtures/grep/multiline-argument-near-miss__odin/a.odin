package probe

Config :: struct { host: string, port: int }

configure :: proc(c: Config) -> string {
    return c.host
}

start_server :: proc() {
    addr := configure(
        Config{
            host = "localhost",
            port = 8080,
        },
    )
    _ = addr
}
