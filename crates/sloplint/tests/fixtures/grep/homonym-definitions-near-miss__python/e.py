from typing import Protocol


class SessionE(Protocol):
    def close(self):
        pass
