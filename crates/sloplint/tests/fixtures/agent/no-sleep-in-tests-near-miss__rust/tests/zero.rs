use std::{thread::sleep, time::Duration as Delay};

#[test]
fn zero_delay_does_not_block() {
    sleep(Delay::ZERO);
    sleep(Delay::from_secs(0));
    sleep(Delay::from_millis(0x0));
    sleep(Delay::from_secs_f64(0.0));
    sleep(Delay::from_secs_f32(-0.0_f32));
    sleep(Delay::from_secs_f64(-((0.0_f64))));
    std::thread::sleep(std::time::Duration::new(0, 0));
}
