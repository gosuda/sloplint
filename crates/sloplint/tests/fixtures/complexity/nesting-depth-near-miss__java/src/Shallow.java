public class Shallow {
    public int f(int[] a) {
        int total = 0;
        for (int x : a) {
            if (x > 0) {
                while (x > 1) {
                    total += x;
                    x -= 1;
                }
            }
        }
        return total;
    }
}
