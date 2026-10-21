function Greet($name) {
  Write-Host $name
  Render-Greeting $name
}
function Render-Greeting($name) {
  Write-Host "hello $name"
}
