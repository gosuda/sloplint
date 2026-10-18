public class Nested {
    public int f(int[] a) {
        int total = 0;
        for (int x : a) {
            if (x > 0) {
                while (x > 1) {
                    if (x > 2) {
                        for (int y : a) {
                            total += x + y;
                        }
                    }
                    x -= 1;
                }
            }
        }
        return total;
    }
}
