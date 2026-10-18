public func load(path: String) -> String {
    do {
        return try readFile(path)
    } catch {
        return "fallback"
    }
}

public func readFile(_ path: String) throws -> String {
    return path
}
