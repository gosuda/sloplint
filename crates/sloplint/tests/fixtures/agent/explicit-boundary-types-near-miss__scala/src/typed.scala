object Api {
  private def helper(data, count) = count

  def typed(data: String, count: Int): Int = data.length + count
}
