import os
COUNTER = 0
def bump():
    global COUNTER
    COUNTER += 1
def load():
    global COUNTER
    COUNTER = len(os.getenv("HOME") or "")
