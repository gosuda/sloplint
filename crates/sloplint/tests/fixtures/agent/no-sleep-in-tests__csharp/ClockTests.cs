using System.Threading;
using Worker = global::System.Threading.Thread;
using static System.Threading.Thread;
class ClockTests {
    void Waits(int millis) {
        Thread.Sleep(1);
        Worker.Sleep(millis);
        System.Threading.Thread.Sleep(-1);
        global::System.Threading.Thread.Sleep(Timeout.Infinite);
        Sleep(1);
        Thread.Sleep(millisecondsTimeout: 1);
    }
}
