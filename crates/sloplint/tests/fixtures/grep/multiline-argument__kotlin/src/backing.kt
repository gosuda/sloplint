class Store {
    val items: List<Int>
        field = buildList(
            [
                1,
                2,
                3,
                4,
                5,
                6,
                7,
                8,
                9,
                10,
                11,
                12,
                13,
                14,
                15,
            ],
        )
}

fun topLevel(): List<Int> {
    return buildList(
        [
            1,
            2,
            3,
            4,
            5,
            6,
            7,
            8,
            9,
            10,
            11,
            12,
            13,
            14,
            15,
        ],
    )
}

fun buildList(body: List<Int>): List<Int> {
    return body
}
