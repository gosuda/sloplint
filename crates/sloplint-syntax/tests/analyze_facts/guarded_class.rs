//! `guarded_class` (`#if`/`#else`-split class head with a shared body) must
//! feed the same facts a `class_specifier` does: the type name, its bases from
//! every preproc arm, member visibility through access markers, and the class
//! container for out-of-line definitions.

use sloplint_core::facts::Visibility;
use sloplint_core::lang::Lang;

use crate::support::run_clean;

const GUARDED: &str = "namespace QtWidgets {
#ifdef PYTHON_BINDINGS
class DOCKS_EXPORT DockWidget : public QWidget, public Core::View
#else
class DOCKS_EXPORT DockWidget : public QtWidgets::View<QWidget>, public Core::View
#endif
{
    Q_OBJECT
public:
    void method() {}
private:
    void helper() {}
};
}
void DockWidget::method2() {}
";

const GUARDED_STRUCT: &str = "#ifdef WIDE
struct Pair : public Base
#else
struct Pair : public Other
#endif
{
    int member_fn() { return 0; }
};
";

#[test]
fn a_guarded_class_yields_its_name_bases_and_members() {
    let analysis = run_clean(Lang::Cpp, "views/dock.h", GUARDED);
    let facts = &analysis.facts;
    let types: Vec<&str> = facts.types.iter().map(|t| t.name).collect();
    assert!(
        types.contains(&"DockWidget"),
        "DockWidget is a type: {types:?}"
    );

    let impls: Vec<&str> = facts.impls.iter().map(|i| i.name).collect();
    for base in ["QWidget", "View"] {
        assert!(
            impls.contains(&base),
            "{base} is an impl of the guarded class: {impls:?}"
        );
    }

    let method = facts
        .functions
        .iter()
        .find(|f| f.name == "method")
        .expect("in-body method definition is a function fact");
    assert!(method.is_method, "DockWidget::method is a class method");
    assert_eq!(
        method.visibility,
        Visibility::Exported,
        "after public: the member is exported"
    );
    let helper = facts
        .functions
        .iter()
        .find(|f| f.name == "helper")
        .expect("in-body helper definition is a function fact");
    assert_eq!(
        helper.visibility,
        Visibility::Private,
        "after private: the member is private"
    );

    let outline = facts
        .functions
        .iter()
        .find(|f| f.name == "method2")
        .expect("out-of-line method definition is a function fact");
    assert!(
        outline.is_method,
        "DockWidget::method2 resolves its class scope"
    );
}

#[test]
fn a_guarded_struct_defaults_members_public() {
    let analysis = run_clean(Lang::Cpp, "pair.h", GUARDED_STRUCT);
    let facts = &analysis.facts;
    let types: Vec<&str> = facts.types.iter().map(|t| t.name).collect();
    assert!(types.contains(&"Pair"), "Pair is a type: {types:?}");
    let impls: Vec<&str> = facts.impls.iter().map(|i| i.name).collect();
    for base in ["Base", "Other"] {
        assert!(impls.contains(&base), "{base} is an impl: {impls:?}");
    }
    let member = facts
        .functions
        .iter()
        .find(|f| f.name == "member_fn")
        .expect("in-body struct member is a function fact");
    assert!(member.is_method);
    assert_eq!(
        member.visibility,
        Visibility::Exported,
        "struct members default public"
    );
}
