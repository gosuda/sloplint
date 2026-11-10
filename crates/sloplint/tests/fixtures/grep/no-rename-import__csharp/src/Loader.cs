using STJ = System.Text.Json;

public class Loader {
    public string Dump(object value) {
        return STJ.JsonSerializer.Serialize(value);
    }
}
