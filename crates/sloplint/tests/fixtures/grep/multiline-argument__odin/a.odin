package probe

Config :: struct { host: string, port: int, mode: string, retries: int }

configure :: proc(c: Config) -> string {
    return c.host
}

start_server :: proc() {
    addr := configure(
        Config{
            host = "localhost",
            port = 8080,
            mode = "production",
            retries = 3,
        },
    )
    _ = addr
}
