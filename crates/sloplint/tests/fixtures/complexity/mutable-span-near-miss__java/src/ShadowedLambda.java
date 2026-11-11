import java.util.List;

public class ShadowedLambda {
    public int spread(int limit) {
        int total = 0;
        total += 1;
        int a01 = 1;
        int a02 = 2;
        int a03 = 3;
        int a04 = 4;
        int a05 = 5;
        int a06 = 6;
        int a07 = 7;
        int a08 = 8;
        int a09 = 9;
        int a10 = 10;
        int a11 = 11;
        int a12 = a10 + a11;
        return total + limit + a12;
    }

    public int tally(List<Integer> items, int p) {
        int[] seen = new int[1];
        items.forEach(total -> {
            total += p;
            seen[0] += total;
        });
        return seen[0];
    }
}
