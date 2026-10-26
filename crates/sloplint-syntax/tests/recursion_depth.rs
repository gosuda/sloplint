//! Adapter helpers that walk nested syntax, observed through `analyze` at 20,000 levels on a
//! default 2 MiB stack.
//!
//! One child module per language, so each fix lands with its own case module and one
//! declaration here, while the crate links a single integration-test executable.

#[path = "recursion_depth/csharp.rs"]
mod csharp;
#[path = "recursion_depth/dart.rs"]
mod dart;
#[path = "recursion_depth/java.rs"]
mod java;
#[path = "recursion_depth/javascript.rs"]
mod javascript;
#[path = "recursion_depth/kotlin.rs"]
mod kotlin;
#[path = "recursion_depth/r.rs"]
mod r;
#[path = "recursion_depth/rust.rs"]
mod rust;
#[path = "recursion_depth/scala.rs"]
mod scala;
#[path = "recursion_depth/starlark.rs"]
mod starlark;
#[path = "recursion_depth/support.rs"]
mod support;
#[path = "recursion_depth/swift.rs"]
mod swift;
