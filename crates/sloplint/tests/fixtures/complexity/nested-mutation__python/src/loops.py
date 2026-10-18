def grid_sum(n):
    total = 0
    for i in range(n):
        for j in range(n):
            for k in range(n):
                total += i + j + k
    return total
