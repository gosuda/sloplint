package clocks

import (
    "testing"
    clock "time"
)

func TestWorker(t *testing.T) {
    clock.Sleep(clock.Millisecond)
}

func waitForWorker() {
    clock.Sleep(10)
}
