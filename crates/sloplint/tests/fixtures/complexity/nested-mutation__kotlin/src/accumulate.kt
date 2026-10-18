fun accumulate(matrix: List<List<Int>>): Int {
    var total = 0
    for (row in matrix) {
        for (x in row) {
            total += x
        }
    }
    return total
}
