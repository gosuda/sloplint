public protocol Renderer {
    func render(verbose: Bool)
}

public class Console: Renderer {
    public func render(verbose: Bool) {
        let _ = verbose
    }
}

private func helper(flag: Bool) -> Bool {
    return !flag
}
