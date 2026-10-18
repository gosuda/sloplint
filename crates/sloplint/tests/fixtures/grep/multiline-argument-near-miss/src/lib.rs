pub fn f() {
    run(
        |x: u64| {
            let y = x + 1;
            let z = y * 2;
            z
        },
    );
}
