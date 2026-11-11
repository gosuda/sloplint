//! Method classification observed through `analyze` only.
//!
//! A function emitted while the walker sits inside a type body is a method; a
//! free function is not.

use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn is_method(lang: Lang, path: &str, source: &str, name: &str) -> bool {
    run_clean(lang, path, source)
        .facts
        .functions
        .iter()
        .find(|f| f.name == name)
        .unwrap_or_else(|| panic!("{path} has function {name}"))
        .is_method
}

#[test]
fn type_body_members_are_methods_free_functions_are_not() {
    assert!(
        is_method(
            Lang::Rust,
            "methods.rs",
            "struct S;\nimpl S {\n    pub fn get(&self) {}\n}\n",
            "get"
        ),
        "impl method is a method"
    );
    assert!(
        !is_method(Lang::Rust, "free.rs", "pub fn get() {}\n", "get"),
        "free function is not a method"
    );
    assert!(
        is_method(
            Lang::Python,
            "methods.py",
            "class C:\n    def get(self):\n        pass\n",
            "get"
        ),
        "class method is a method"
    );
    assert!(
        !is_method(Lang::Python, "free.py", "def get():\n    pass\n", "get"),
        "free function is not a method"
    );
}

#[test]
fn go_receiver_functions_are_methods_plain_functions_are_not() {
    assert!(
        is_method(
            Lang::Go,
            "buffer.go",
            "package probe\nfunc (b *Buffer) Write(p []byte) {}\n",
            "Write"
        ),
        "method with a receiver is a method"
    );
    assert!(
        !is_method(
            Lang::Go,
            "write.go",
            "package probe\nfunc Write() {}\n",
            "Write"
        ),
        "receiverless function is not a method"
    );
}

#[test]
fn function_end_line_is_the_last_line_of_the_node() {
    let analysis = run_clean(Lang::Rust, "span.rs", "\nfn f() {\n    1\n}\n");
    let end = analysis
        .facts
        .functions
        .iter()
        .find(|f| f.name == "f")
        .unwrap_or_else(|| panic!("span.rs has function f"))
        .end_line;
    assert_eq!(
        end, 4,
        "three-line function starting at line 2 ends at line 4"
    );
}
