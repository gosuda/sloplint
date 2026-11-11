def tally(items, p):
    total = 0
    total += 1
    a01 = 1
    a02 = 2
    a03 = 3
    a04 = 4
    a05 = 5
    a06 = 6
    a07 = 7
    a08 = 8
    a09 = 9
    a10 = 10
    a11 = a09 + a10
    a12 = a11 + p
    a13 = a12 + 1
    def helper(total):
        total += a13
        return total
    return total + helper(p)
