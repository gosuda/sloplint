public class Classifier {
    public String classify(int code) {
        return switch (code) {
            case 0 -> "zero";
            case 1 -> "one";
            default -> "many";
        };
    }
}
