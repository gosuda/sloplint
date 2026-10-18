public class Checks {
    public void checkReady(boolean ready) {
        assert ready : "cache must be warm";
    }
}
