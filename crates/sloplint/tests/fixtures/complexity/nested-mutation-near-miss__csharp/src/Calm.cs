public class Calmer {
    public int SumRow(int[] xs) {
        int total = 0;
        foreach (int x in xs) {
            total += x;
        }
        return total;
    }
}
