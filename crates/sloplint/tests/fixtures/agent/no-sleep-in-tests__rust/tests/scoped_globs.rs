mod unrelated {
    use simulated::*;
}

#[test]
fn waits_outside_glob_scope() {
    ::std::thread::sleep(std::time::Duration::from_millis(1));
    std::thread::sleep(::std::time::Duration::from_millis(1));
}
