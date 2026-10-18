public class Store {
    private static int counter;
    public void Bump() {
        counter++;
    }
    public int Read() {
        return counter;
    }
}
