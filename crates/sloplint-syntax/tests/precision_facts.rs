//! New adapter facts for the precision wave, one child module per language, observed
//! through `analyze` only.
#[path = "precision_facts/bash.rs"]
mod bash;
#[path = "precision_facts/c.rs"]
mod c;
#[path = "precision_facts/cpp.rs"]
mod cpp;
#[path = "precision_facts/cpp_overrides.rs"]
mod cpp_overrides;
#[path = "precision_facts/csharp.rs"]
mod csharp;
#[path = "precision_facts/csharp_extra.rs"]
mod csharp_extra;
#[path = "precision_facts/dart.rs"]
mod dart;
#[path = "precision_facts/dart_shadow.rs"]
mod dart_shadow;
#[path = "precision_facts/go.rs"]
mod go;
#[path = "precision_facts/go_callee.rs"]
mod go_callee;
#[path = "precision_facts/go_shapes.rs"]
mod go_shapes;
#[path = "precision_facts/java.rs"]
mod java;
#[path = "precision_facts/java_calls.rs"]
mod java_calls;
#[path = "precision_facts/javascript.rs"]
mod javascript;
#[path = "precision_facts/javascript_exports.rs"]
mod javascript_exports;
#[path = "precision_facts/kotlin.rs"]
mod kotlin;
#[path = "precision_facts/kotlin_contract.rs"]
mod kotlin_contract;
#[path = "precision_facts/objc.rs"]
mod objc;
#[path = "precision_facts/ocaml.rs"]
mod ocaml;
#[path = "precision_facts/ocaml_contract.rs"]
mod ocaml_contract;
#[path = "precision_facts/ocaml_review.rs"]
mod ocaml_review;
#[path = "precision_facts/odin.rs"]
mod odin;
#[path = "precision_facts/odin_contract.rs"]
mod odin_contract;
#[path = "precision_facts/odin_contract2.rs"]
mod odin_contract2;
#[path = "precision_facts/php.rs"]
mod php;
#[path = "precision_facts/php_contract.rs"]
mod php_contract;
#[path = "precision_facts/powershell.rs"]
mod powershell;
#[path = "precision_facts/powershell_args.rs"]
mod powershell_args;
#[path = "precision_facts/powershell_shapes.rs"]
mod powershell_shapes;
#[path = "precision_facts/powershell_strings.rs"]
mod powershell_strings;
#[path = "precision_facts/python.rs"]
mod python;
#[path = "precision_facts/r.rs"]
mod r;
#[path = "precision_facts/r_round3.rs"]
mod r_round3;
#[path = "precision_facts/ruby.rs"]
mod ruby;
#[path = "precision_facts/ruby_contract.rs"]
mod ruby_contract;
#[path = "precision_facts/ruby_contract_extra.rs"]
mod ruby_contract_extra;
#[path = "precision_facts/rust.rs"]
mod rust;
#[path = "precision_facts/rust_trait_impl.rs"]
mod rust_trait_impl;
#[path = "precision_facts/scala.rs"]
mod scala;
#[path = "precision_facts/scala_calls.rs"]
mod scala_calls;
#[path = "precision_facts/scala_contract.rs"]
mod scala_contract;
#[path = "precision_facts/starlark.rs"]
mod starlark;
#[path = "precision_facts/starlark_calls.rs"]
mod starlark_calls;
#[path = "precision_facts/starlark_shapes.rs"]
mod starlark_shapes;
#[path = "precision_facts/swift.rs"]
mod swift;
#[path = "precision_facts/typescript.rs"]
mod typescript;
#[path = "precision_facts/zig.rs"]
mod zig;
