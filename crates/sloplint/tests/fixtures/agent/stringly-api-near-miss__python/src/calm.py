from typing import overload


def serve(a: str, *, b: str, c: str):
    return a + b + c


@overload
def fetch(a: str, b: str, c: str) -> str: ...


@overload
def fetch(a: int) -> int: ...


def fetch(a, b=None, c=None):
    return a
