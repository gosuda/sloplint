pub fn check_ready(ready: bool) !void {
    if (!ready) {
        return error.NotReady;
    }
}
