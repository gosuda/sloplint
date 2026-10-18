public func renderTable(header: Bool, footer: Bool) -> String {
    if header {
        return "h"
    }
    if footer {
        return "f"
    }
    return ""
}
