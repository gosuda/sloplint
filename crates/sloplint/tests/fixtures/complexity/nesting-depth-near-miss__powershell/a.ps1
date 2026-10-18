function Invoke-Shallow($a) {
  foreach ($i in 1..2) {
    if ($a) {
      while ($a) {
        $a
      }
    }
  }
}
