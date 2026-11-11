from typing import Protocol


class SessionC(Protocol):
    def close(self):
        pass
