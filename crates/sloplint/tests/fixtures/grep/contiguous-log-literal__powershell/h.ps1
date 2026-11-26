function Invoke-Log($manifest) {
  Write-Host v="$($manifest.PrivateData.PSData.Prerelease)"
}
