pub static mut COUNTER: u64 = 0;

pub fn bump() {
    unsafe { COUNTER += 1; }
}

pub fn load() {
    unsafe { COUNTER = std::env::var("HOME").unwrap_or_default().len() as u64; }
}
