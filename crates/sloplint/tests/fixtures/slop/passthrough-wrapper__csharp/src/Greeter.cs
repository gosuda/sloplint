public class Greeter {
    public string Greet(string name, bool loud) {
        return RenderGreeting(name, loud);
    }

    private string RenderGreeting(string name, bool loud) {
        if (loud) {
            return name.ToUpper();
        }
        return name;
    }
}
