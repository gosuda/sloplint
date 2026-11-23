use std::{thread::{self, sleep as pause}, time::Duration};

#[test]
fn worker_finishes() {
    thread::sleep(Duration::from_millis(1));
    pause(Duration::from_secs(1));
    pause(Duration::ZERO);
}
