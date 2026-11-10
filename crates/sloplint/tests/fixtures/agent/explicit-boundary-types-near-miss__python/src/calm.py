def _process(data, count):
    return len(data) + count


def typed(data: str, count: int) -> int:
    return len(data) + count


def main():
    print(1)


class Url:
    def __new__(cls: type, host: str) -> "Url":
        obj = super().__new__(cls)
        obj.host = host
        return obj
