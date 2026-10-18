pub enum Status {
    Active,
    Idle,
    Closed,
}

pub fn describe(status: Status) -> &'static str {
    match status {
        Status::Active => "active",
        Status::Idle => "idle",
        _ => "other",
    }
}
