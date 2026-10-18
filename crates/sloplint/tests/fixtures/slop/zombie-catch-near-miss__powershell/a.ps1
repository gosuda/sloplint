function Invoke-Safe($path) {
  try {
    Get-Item $path
  } catch {
    throw "context: $_"
  }
}
