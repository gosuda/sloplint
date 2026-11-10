public class Modes {
    public string Describe(Mode mode) {
        switch (mode) {
            case Mode.Fast:
                return "fast";
            case Mode.Slow:
                return "slow";
            default:
                return "other";
        }
    }
}

public enum Mode {
    Fast,
    Slow,
}
