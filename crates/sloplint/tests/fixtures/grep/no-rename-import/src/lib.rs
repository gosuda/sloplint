use std::collections::HashMap as Table;
use std::fmt::Result as Outcome;

pub fn lookup(map: &Table<String, u64>) -> Option<u64> {
    map.get("k").copied()
}
