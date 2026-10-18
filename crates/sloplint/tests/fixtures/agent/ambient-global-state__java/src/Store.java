public class Store {
    public static int counter = 0;

    public String serviceHome() {
        return System.getenv("HOME");
    }
}
