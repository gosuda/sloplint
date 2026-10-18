public class Grids {
    public int GridSum(int n) {
        int total = 0;
        for (int i = 0; i < n; i++) {
            for (int j = 0; j < n; j++) {
                for (int k = 0; k < n; k++) {
                    total += i + j + k;
                }
            }
        }
        return total;
    }
}
