using System;

public class Loader {
    public object? Load(Type type) {
        return type.GetMethod("Load");
    }
}
