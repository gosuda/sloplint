public class Store {
    public static int counter = 0;
    public void bump() {
        counter++;
    }
    public void load() {
        counter = System.getenv("HOME") != null ? 1 : 0;
    }
}
