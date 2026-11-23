import builtins as runtime
import time

def fake_sleep(delay: float) -> None:
    pass

def test_virtual_sleep() -> None:
    original = time.sleep
    try:
        runtime.setattr(time, "sleep", fake_sleep)
        time.sleep(1)
    finally:
        runtime.setattr(time, "sleep", original)
