from typing import Protocol


class SessionD(Protocol):
    def close(self):
        pass
