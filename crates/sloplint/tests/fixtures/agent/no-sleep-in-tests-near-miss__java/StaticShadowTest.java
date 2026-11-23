import static java.lang.Thread.sleep;
final class StaticShadowTest {
    void sleep(int millis) {}
    void advancesClock() {
        sleep(10);
    }
}
