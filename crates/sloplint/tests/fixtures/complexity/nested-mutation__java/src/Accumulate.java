public class Accumulate {
    public int compute(int[] rows, int[] cols, int[] deps) {
        int total = 0;
        for (int r : rows) {
            for (int c : cols) {
                for (int d : deps) {
                    total += r + c + d;
                }
            }
        }
        return total;
    }
}
