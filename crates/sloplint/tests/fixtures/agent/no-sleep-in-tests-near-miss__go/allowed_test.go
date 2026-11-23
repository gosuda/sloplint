package clocks
import "time"
func waitForClockTick() {
    // sloplint-allow(agent/no-sleep-in-tests): this integration test observes the OS clock
    time.Sleep(time.Millisecond)
}
