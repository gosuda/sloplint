mod simulated {
    pub mod thread {
        pub fn sleep(_: u64) {}
    }
}
use simulated as std;

#[test]
fn virtual_clock() {
    std::thread::sleep(1);
}
