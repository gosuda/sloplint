def f(items):
    total = 0
    for row in items:
        for col in row:
            for cell in col:
                total += 1
    return total
