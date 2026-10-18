import logging

logger = logging.getLogger(__name__)


def load(path):
    try:
        return open(path).read()
    except OSError:
        logger.error("load failed")
        return None
