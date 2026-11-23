using JS = Newtonsoft.Json;

public class Loader {
    public string Dump(object value) {
        System.Func<int, int, int> f = (ref Json, out count) => Json;
        return JS.Serialize(value);
    }
}
