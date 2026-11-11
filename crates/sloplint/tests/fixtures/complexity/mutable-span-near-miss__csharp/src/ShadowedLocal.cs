public class ShadowedLocal {
    public int Spread(int limit) {
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
        int a11 = a09 + a10;
        return total + limit + a11;
    }

    public int Tally(System.Collections.Generic.List<int> items, int p) {
        int total = p;
        int q = total + 1;
        int r = q + 2;
        int s = r + 3;
        int t = s + 4;
        total += t;
        return total + items.Count;
    }
}
