function Invoke-Accum($items) {
  $total = 0
  foreach ($a in $items) {
    foreach ($b in $items) {
      foreach ($c in $items) {
        $total++
      }
    }
  }
  $total
}
