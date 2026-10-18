public class Classifier {
    public string Classify(int n) {
        int r = 0;
        if (n == 0) { r += 1; }
        if (n == 1) { r += 2; }
        if (n == 2) { r += 3; }
        if (n == 3) { r += 4; }
        if (n == 4) { r += 5; }
        if (n == 5) { r += 6; }
        if (n == 6) { r += 7; }
        if (n == 7) { r += 8; }
        if (n == 8) { r += 9; }
        if (n == 9) { r += 10; }
        if (n == 10) { r += 11; }
        if (n == 11) { r += 12; }
        if (n == 12) { r += 13; }
        if (n == 13) { r += 14; }
        if (n == 14) { r += 15; }
        if (n == 15) { r += 16; }
        return r.ToString();
    }
}
