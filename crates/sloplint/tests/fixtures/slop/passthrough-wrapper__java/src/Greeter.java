public class Greeter {
    private Renderer renderer;

    public String greet(String name) {
        return renderer.render(name);
    }
}
