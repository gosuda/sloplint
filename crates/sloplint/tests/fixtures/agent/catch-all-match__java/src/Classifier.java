public class Classifier {
    public enum Color {
        RED, GREEN, BLUE
    }

    public String describe(Color color) {
        return switch (color) {
            case RED -> "red";
            case GREEN -> "green";
            default -> "other";
        };
    }
}
