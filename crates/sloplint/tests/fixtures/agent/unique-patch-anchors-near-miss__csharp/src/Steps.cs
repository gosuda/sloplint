public class Steps {
    public void StepFirst(int input, ref int output) {
        int alpha = ComputeFirst(input);
        int beta = ComputeSecond(alpha);
        int gamma = ComputeThird(beta);
        int delta = ComputeFourth(gamma);
        int epsilon = ComputeFifth(delta);
        StoreResult(ref output, epsilon);
        FinishFirst();
    }

    public void StepSecond(int input, ref int output) {
        int alpha = ComputeFirst(input);
        int beta = ComputeSecond(alpha);
        int gamma = ComputeThird(beta);
        int delta = ComputeFourth(gamma);
        int epsilon = ComputeFifth(delta);
        StoreResult(ref output, epsilon);
        FinishSecond();
    }

    private int ComputeFirst(int v) {
        return v;
    }
}
