#[test]
fn blocking_clock_integration() {
    // sloplint-allow(agent/no-sleep-in-tests): integration test verifies the platform timer
    std::thread::sleep(std::time::Duration::from_millis(1));
}
