public class Loader {
    public Class<?> load(String className) throws ClassNotFoundException {
        return Class.forName(className);
    }
}
