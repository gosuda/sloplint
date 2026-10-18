public class Sum {
    public int total(int[] items) {
        int total = 0;
        for (int item : items) {
            total += item;
        }
        return total;
    }
}
