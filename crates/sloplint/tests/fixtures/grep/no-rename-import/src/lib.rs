use std::collections::HashMap as Map;
use std::fmt::Result as FmtResult;

pub fn lookup(map: &Map<String, u64>) -> Option<u64> {
    map.get("k").copied()
}
