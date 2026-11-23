final class ClockTest {
    void yields() throws InterruptedException {
        Thread.sleep(0);
        Thread.sleep((0L));
        Thread.sleep(+0);
        Thread.sleep(0x0);
        Thread.sleep(0b0);
        Thread.sleep(0, 0);
        java.lang.Thread.sleep(/* yield */ (0));
        Thread.sleep(-1);
        Thread.sleep(0, -1);
        Thread.sleep(0, 1_000_000);
    }
}
