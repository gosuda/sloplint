public enum Status {
    case active
    case idle
    case closed
}

public func describe(status: Status) -> String {
    switch status {
    case .active:
        return "active"
    case .idle:
        return "idle"
    default:
        return "other"
    }
}
