function Invoke-Empty($path) {
  try {
    Get-Item $path
  } catch {
  }
}
