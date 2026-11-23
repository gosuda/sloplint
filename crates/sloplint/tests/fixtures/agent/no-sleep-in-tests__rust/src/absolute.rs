#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn waits() {
        ::std::thread::sleep(::std::time::Duration::from_millis(1));
        ::std::thread::sleep(::std::time::Duration::ZERO);
    }
}
