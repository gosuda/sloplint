function Invoke-Count($items) {
  $global:Total += 1
  $home = $env:HOME
  $items
}
