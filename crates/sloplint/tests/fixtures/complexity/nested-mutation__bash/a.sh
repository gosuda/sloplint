grid_sum() {
  total=0
  for i in 1 2 3; do
    for j in 1 2 3; do
      for k in 1 2 3; do
        total=$((total + 1))
      done
    done
  done
  echo "$total"
}
