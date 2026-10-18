function Invoke-Accum($items) {
  $total = 0
  foreach ($a in $items) {
    $total++
  }
  $total
}
