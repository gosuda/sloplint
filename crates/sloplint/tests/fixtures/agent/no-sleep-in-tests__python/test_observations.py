import logging
import time
from time import sleep as pause

def test_worker_observations() -> None:
    events: dict[float, str] = {}
    logger = logging.getLogger(__name__)
    logger.debug("time.sleep")
    events[time.monotonic()] = "started"
    time.sleep(0.1)
    pause(0.2)
