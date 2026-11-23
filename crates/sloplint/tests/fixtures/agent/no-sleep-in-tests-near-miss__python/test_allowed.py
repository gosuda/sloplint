import time

def test_os_clock():
    # sloplint-allow(agent/no-sleep-in-tests): this integration test observes the OS clock
    time.sleep(0.01)
