mod unrelated {
    use simulated::{nested::{*}};
}

#[test]
fn zero_delays_outside_glob_scope() {
    { use simulated::*; }
    ::std::thread::sleep(std::time::Duration::ZERO);
    ::std::thread::sleep(std::time::Duration::from_millis(0));
    std::thread::sleep(::std::time::Duration::ZERO);
    std::thread::sleep(std::time::Duration::new(0, 0));
}

mod outer {
    use simulated::*;
    mod inner {
        #[test]
        fn zero_delay_without_inherited_glob() {
            ::std::thread::sleep(std::time::Duration::ZERO);
        }
    }
}
