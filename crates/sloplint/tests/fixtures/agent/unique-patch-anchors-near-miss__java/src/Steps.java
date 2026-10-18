public class Steps {
    public void stepFirst(int input) {
        int alpha = computeFirst(input);
        int beta = computeSecond(alpha);
        int gamma = computeThird(beta);
        int delta = computeFourth(gamma);
        int epsilon = computeFifth(delta);
        storeResult(epsilon);
    }

    public void stepSecond(int input) {
        int alpha = computeFirst(input);
        int beta = computeSecond(alpha);
        int gamma = computeThird(beta);
        int delta = computeFourth(gamma);
        int epsilon = computeFifth(delta);
        storeResult(epsilon);
    }

    public int computeFirst(int v) { return v; }
    public int computeSecond(int v) { return v; }
    public int computeThird(int v) { return v; }
    public int computeFourth(int v) { return v; }
    public int computeFifth(int v) { return v; }
    public void storeResult(int v) { }
}
