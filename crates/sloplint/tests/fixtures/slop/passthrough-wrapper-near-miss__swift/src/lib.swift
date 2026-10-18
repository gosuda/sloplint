public func greet(name: String, loud: Bool) -> String {
    return renderGreeting(loud, name)
}

public func renderGreeting(_ loud: Bool, _ name: String) -> String {
    return name
}
