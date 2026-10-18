sum_row() {
  total=0
  for i in 1 2 3; do
    total=$((total + 1))
  done
  echo "$total"
}
