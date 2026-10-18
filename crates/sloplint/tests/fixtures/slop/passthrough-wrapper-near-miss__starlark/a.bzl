def inner(x, y):
    return x + y

def wrapper(x, y):
    return inner(y, x)
