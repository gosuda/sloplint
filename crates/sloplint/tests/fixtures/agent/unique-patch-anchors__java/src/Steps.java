public class Steps {
    public void stepFirst(int input) {
        int alpha = computeFirst(input);
        int beta = computeSecond(alpha);
        int gamma = computeThird(beta);
        int delta = computeFourth(gamma);
        int epsilon = computeFifth(delta);
        storeResult(epsilon);
        finishFirst();
    }

    public void stepSecond(int input) {
        int alpha = computeFirst(input);
        int beta = computeSecond(alpha);
        int gamma = computeThird(beta);
        int delta = computeFourth(gamma);
        int epsilon = computeFifth(delta);
        storeResult(epsilon);
        finishSecond();
    }

    public void stepThird(int input) {
        int alpha = computeFirst(input);
        int beta = computeSecond(alpha);
        int gamma = computeThird(beta);
        int delta = computeFourth(gamma);
        int epsilon = computeFifth(delta);
        storeResult(epsilon);
        finishThird();
    }

    public int computeFirst(int v) { return v; }
    public int computeSecond(int v) { return v; }
    public int computeThird(int v) { return v; }
    public int computeFourth(int v) { return v; }
    public int computeFifth(int v) { return v; }
    public void storeResult(int v) { }
    public void finishFirst() { }
    public void finishSecond() { }
    public void finishThird() { }
}
