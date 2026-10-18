function Invoke-Deep($a) {
  foreach ($i in 1..2) {
    if ($a) {
      while ($a) {
        switch ($a) {
          default { if ($a) { $a } }
        }
      }
    }
  }
}
