public class Loader {
    public String load(String path) {
        try {
            return read(path);
        } catch (Exception e) {
            return "unknown";
        }
    }

    public String read(String path) {
        return path;
    }
}
