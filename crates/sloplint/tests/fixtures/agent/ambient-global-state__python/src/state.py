import os


def bump():
    global COUNTER
    COUNTER += 1


COUNTER = 0


def service_home():
    return os.getenv("HOME")
