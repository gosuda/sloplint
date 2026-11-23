#[test]
fn async_timer_values() {
    let _tokio = tokio::time::sleep(std::time::Duration::from_secs(1));
    let _async_std = async_std::task::sleep(std::time::Duration::from_secs(1));
}
