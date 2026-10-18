public class Nester {
    public int Run(int[] xs, int c, int d) {
        int total = 0;
        foreach (int x in xs) {
            if (x > 0) {
                while (total < 100) {
                    switch (c) {
                        case 1:
                            if (d > 0) {
                                total += x;
                            }
                            break;
                        default:
                            break;
                    }
                    total += 1;
                }
            }
        }
        return total;
    }
}
