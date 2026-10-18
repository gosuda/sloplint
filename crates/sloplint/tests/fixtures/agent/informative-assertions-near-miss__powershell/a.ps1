function Invoke-Check($x) {
  $x | Should -Be 1 -Because "cache must be warm"
}
