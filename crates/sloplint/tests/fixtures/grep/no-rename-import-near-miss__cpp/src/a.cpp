// near-miss: named import without alias.
using std::string;
string f() { return string(); }

// near-miss: the alias target name is bound by a member-pointer typedef —
// bound_names reads facts.types, so `fetch` is already taken.
struct Hooks {};
typedef void (Hooks::*fetch)();
namespace g = util::fetch;

// near-miss: the alias target name is bound by a guarded class —
// the `#if`-split class head lands in facts.types.
#ifdef PY
class EXPORT DockWidget : public Base
#else
class EXPORT DockWidget : public Other
#endif
{
public:
    void method() {}
};
namespace dw = ui::DockWidget;
