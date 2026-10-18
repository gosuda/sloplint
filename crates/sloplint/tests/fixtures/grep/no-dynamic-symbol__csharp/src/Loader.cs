using System;

public class Loader {
    public object? Load(Type type, string name) {
        return type.GetMethod(name);
    }
}
