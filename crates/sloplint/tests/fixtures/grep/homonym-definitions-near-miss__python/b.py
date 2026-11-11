from typing import Protocol


class SessionB(Protocol):
    def close(self):
        pass
