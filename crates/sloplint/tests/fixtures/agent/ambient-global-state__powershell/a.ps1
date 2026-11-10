$global:Total = 0
function Bump-Count {
  $global:Total += 1
}
function Import-Home {
  $global:Total = $env:HOME.Length
}
