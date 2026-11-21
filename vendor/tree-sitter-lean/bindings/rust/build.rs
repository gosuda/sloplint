fn main() {
    let src_dir = std::path::Path::new("src");
    let out_dir = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap());

    let mut c_config = cc::Build::new();
    c_config.std("c11").include(src_dir);

    #[cfg(target_env = "msvc")]
    c_config.flag("-utf-8");

    // The generated parser exceeds GitHub's 100MB blob limit, so the
    // committed artifact is parser.c.gz. A fresh regen produces src/parser.c
    // (gitignored); builds decompress the packed copy into OUT_DIR when the
    // plain file is absent.
    let parser_path = src_dir.join("parser.c");
    let parser_gz = src_dir.join("parser.c.gz");
    let parser_path = if parser_path.exists() {
        parser_path
    } else {
        let packed = std::fs::File::open(&parser_gz)
            .expect("src/parser.c.gz missing: regenerate the vendored grammar");
        let mut dec = flate2::read::GzDecoder::new(packed);
        let out = out_dir.join("parser.c");
        let mut f = std::fs::File::create(&out).unwrap();
        std::io::copy(&mut dec, &mut f).unwrap();
        out
    };
    c_config.file(&parser_path);
    println!("cargo:rerun-if-changed={}", parser_path.to_str().unwrap());
    println!("cargo:rerun-if-changed={}", parser_gz.to_str().unwrap());

    let scanner_path = src_dir.join("scanner.c");
    c_config.file(&scanner_path);
    println!("cargo:rerun-if-changed={}", scanner_path.to_str().unwrap());

    c_config.compile("tree-sitter-lean");
}
