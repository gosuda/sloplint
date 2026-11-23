import java.lang.Thread;
import static java.lang.Thread.sleep;
final class ClockTest {
    void waits(long millis, int nanos) throws InterruptedException {
        Thread.sleep(millis);
        Thread.sleep(0, nanos);
        java.lang.Thread.sleep(1);
        sleep(0, 1);
    }
}
