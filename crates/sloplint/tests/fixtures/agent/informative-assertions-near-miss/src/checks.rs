pub fn check_ready(ready: bool) {
    assert!(ready, "service must be ready before serving traffic");
}
