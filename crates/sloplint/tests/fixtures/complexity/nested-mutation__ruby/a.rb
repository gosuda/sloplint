def accumulate(matrix)
  total = 0
  matrix.each do |row|
    row.each do |x|
      total += x
    end
  end
  total
end
