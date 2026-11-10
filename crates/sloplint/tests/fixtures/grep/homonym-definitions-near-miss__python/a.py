from typing import Protocol


class SessionA(Protocol):
    def close(self):
        pass
