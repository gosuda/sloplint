use sloplint_core::lang::Lang;

use crate::support::run_clean;

fn closure_param_names(source: &str) -> Vec<&str> {
    let analysis = run_clean(Lang::CSharp, "case.cs", source);
    let closures: Vec<_> = analysis
        .facts
        .functions
        .iter()
        .filter(|found| found.name.is_empty())
        .collect();
    assert_eq!(
        closures.len(),
        1,
        "case.cs keeps one closure function, found {}",
        closures.len()
    );
    closures[0].params.iter().map(|param| param.name).collect()
}

#[test]
fn modifier_lambda_keeps_every_parameter() {
    let source = "class A {\nvoid M() {\nvar f = (ref x, out y) => x;\n}\n}\n";
    assert_eq!(
        closure_param_names(source),
        ["x", "y"],
        "the modifier lambda keeps both parameters"
    );
}

#[test]
fn scoped_modifier_lambda_keeps_its_parameter() {
    let source = "class A {\nvoid M() {\nvar f = (scoped ref a) => a;\n}\n}\n";
    assert_eq!(
        closure_param_names(source),
        ["a"],
        "the scoped modifier lambda keeps its parameter"
    );
}

#[test]
fn plain_lambda_keeps_its_parameter() {
    let source = "class A {\nvoid M() {\nSystem.Action<int> f = x => { x = 1; };\n}\n}\n";
    assert_eq!(
        closure_param_names(source),
        ["x"],
        "the plain lambda keeps its parameter"
    );
}

#[test]
fn typed_lambda_keeps_both_parameters() {
    let source =
        "class A {\nvoid M() {\nSystem.Func<int, int, int> f = (int x, int y) => x + y;\n}\n}\n";
    assert_eq!(
        closure_param_names(source),
        ["x", "y"],
        "the typed lambda keeps both parameters"
    );
}
