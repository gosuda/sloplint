public class Shallow {
    public int Run(int[] xs) {
        int total = 0;
        foreach (int x in xs) {
            if (x > 0) {
                while (total < 100) {
                    total += x;
                }
            }
        }
        return total;
    }
}
