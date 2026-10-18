public class Calm {
    public int classify(int n) {
        int x = n;
        if (n == 0) { x += 1; }
        if (n == 1) { x += 2; }
        if (n == 2) { x += 3; }
        if (n == 3) { x += 4; }
        if (n == 4) { x += 5; }
        if (n == 5) { x += 6; }
        if (n == 6) { x += 7; }
        if (n == 7) { x += 8; }
        if (n == 8) { x += 9; }
        return x;
    }
}
