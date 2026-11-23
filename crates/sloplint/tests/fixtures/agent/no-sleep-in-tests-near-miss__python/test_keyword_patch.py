import time
from unittest.mock import Mock, patch

@patch(target="time.sleep")
def test_virtual_sleep(fake_sleep: Mock) -> None:
    time.sleep(1)
    fake_sleep.assert_called_once_with(1)
