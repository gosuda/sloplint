public class Modes {
    public string Describe(Mode mode) {
        switch (mode) {
            case Mode.Fast:
                return "fast";
            default:
                return "other";
        }
    }
}

public enum Mode {
    Fast,
    Slow,
}
