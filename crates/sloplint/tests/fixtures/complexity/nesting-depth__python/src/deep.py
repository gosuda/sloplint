def nested(a, b, c, d, e):
    for _ in range(1):
        if a:
            for _ in range(2):
                if b:
                    if c:
                        return 1
    return 0
