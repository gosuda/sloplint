//! ETXTBSY-tolerant child spawn shared by cache tests.
use std::io;
use std::process::Output;
use std::time::Duration;

#[cfg(unix)]
pub fn spawn_forgiving(mut spawn: impl FnMut() -> io::Result<Output>) -> Output {
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let mut last_busy = None;
    while std::time::Instant::now() < deadline {
        match spawn() {
            Ok(out) => return out,
            Err(e) if e.raw_os_error() == Some(26) => {
                last_busy = Some(e);
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => panic!("spawn: {e}"),
        }
    }
    panic!(
        "spawn stayed ETXTBSY for 5 s: {}",
        last_busy.map_or("unknown".to_string(), |e| e.to_string())
    )
}
