using System.Text.Json;

public class Loader {
    public string Dump(object value) {
        return JsonSerializer.Serialize(value);
    }
}
