from abc import ABC, abstractmethod


class _Shape(ABC):
    @abstractmethod
    def draw(self):
        pass
