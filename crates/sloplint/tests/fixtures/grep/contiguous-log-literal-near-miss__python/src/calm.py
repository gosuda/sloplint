import logging

logger = logging.getLogger(__name__)


def load(path, err):
    logger.error(f"failed to open {path}: {err}")
