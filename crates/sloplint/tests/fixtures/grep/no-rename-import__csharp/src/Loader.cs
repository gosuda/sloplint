using J = System.Text.Json;

public class Loader {
    public string Dump(object value) {
        return J.JsonSerializer.Serialize(value);
    }
}
