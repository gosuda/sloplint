fn backoff(delay: std::time::Duration) {
    std::thread::sleep(delay);
}

#[test]
fn index_appears() {
    std::thread::sleep(std::time::Duration::from_millis(1));
}

#[cfg(test)]
mod checks {
    #[test]
    fn index_appears() {
        ::std::thread::sleep(::std::time::Duration::from_millis(1));
    }
}
