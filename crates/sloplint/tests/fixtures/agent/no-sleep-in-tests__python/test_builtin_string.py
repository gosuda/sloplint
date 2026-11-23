import builtins
import time

def test_string_object() -> None:
    try:
        builtins.setattr("time.sleep", "attribute", None)
    except AttributeError:
        pass
    time.sleep(0.1)
