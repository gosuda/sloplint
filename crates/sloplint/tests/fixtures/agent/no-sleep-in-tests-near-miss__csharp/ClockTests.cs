using System.Threading;
using System.Threading.Tasks;
class ClockTests {
    void Yields() {
        Thread.Sleep(0);
        Thread.Sleep((0));
        Thread.Sleep(+0);
        Thread.Sleep(-0);
        Thread.Sleep(0x0);
        Thread.Sleep(0b0);
        Thread.Sleep(millisecondsTimeout: /* yield */ (0));
        Task.Delay(10);
        Thread.Sleep(-2);
        Thread.Sleep(millisecondsTimeout: -2147483648);
    }
}
