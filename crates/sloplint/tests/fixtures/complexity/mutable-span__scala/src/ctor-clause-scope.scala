object CtorClauseScope {
  class Sink {
    def apply(count: Int): Int = count
  }
  def sink(other: Int): Int = 0
  // Early: `sink` is a parameter of the first clause, which ends before the
  // call in the second clause's default, so the call resolves through the
  // parameter's `apply` and records no write. The helper keeps the
  // long-lived `count` observable: constructor defaults are not function
  // scope, so a bare block there binds nothing.
  class Early(sink: Sink)(value: Int = {
    def helper(): Int = {
      var count = 0
      count = count + 1
      val a01 = 1
      val a02 = 2
      val a03 = 3
      val a04 = 4
      val a05 = 5
      val a06 = 6
      val a07 = 7
      val a08 = 8
      val a09 = 9
      val a10 = 10
      val a11 = 11
      val a12 = 12
      val a13 = 13
      val a14 = 14
      val a15 = 15
      val a16 = 16
      val a17 = 17
      val a18 = 18
      val a19 = 19
      val a20 = 20
      val a21 = 21
      val a22 = 22
      val a23 = 23
      val a24 = 24
      val a25 = 25
      val a26 = 26
      val a27 = 27
      val a28 = 28
      val a29 = 29
      sink(count = 1)
      count
    }
    helper()
  })
  // Same: `sink` shares the call's own clause, where a parameter is out of
  // scope, so the call resolves to the outer method, which lacks `count`,
  // and the assignment writes `count`.
  class Same(sink: Sink, value: Int = {
    def helper(): Int = {
      var count = 0
      count = count + 1
      val a01 = 1
      val a02 = 2
      val a03 = 3
      val a04 = 4
      val a05 = 5
      val a06 = 6
      val a07 = 7
      val a08 = 8
      val a09 = 9
      val a10 = 10
      val a11 = 11
      val a12 = 12
      val a13 = 13
      val a14 = 14
      val a15 = 15
      val a16 = 16
      val a17 = 17
      val a18 = 18
      val a19 = 19
      val a20 = 20
      val a21 = 21
      val a22 = 22
      val a23 = 23
      val a24 = 24
      val a25 = 25
      val a26 = 26
      val a27 = 27
      val a28 = 28
      val a29 = 29
      sink(count = 1)
      count
    }
    helper()
  })
  // Late: `sink` comes in a later clause, which is out of scope at the
  // call, so the call resolves to the outer method and writes `count`.
  class Late(value: Int = {
    def helper(): Int = {
      var count = 0
      count = count + 1
      val a01 = 1
      val a02 = 2
      val a03 = 3
      val a04 = 4
      val a05 = 5
      val a06 = 6
      val a07 = 7
      val a08 = 8
      val a09 = 9
      val a10 = 10
      val a11 = 11
      val a12 = 12
      val a13 = 13
      val a14 = 14
      val a15 = 15
      val a16 = 16
      val a17 = 17
      val a18 = 18
      val a19 = 19
      val a20 = 20
      val a21 = 21
      val a22 = 22
      val a23 = 23
      val a24 = 24
      val a25 = 25
      val a26 = 26
      val a27 = 27
      val a28 = 28
      val a29 = 29
      sink(count = 1)
      count
    }
    helper()
  })(sink: Sink)
}
