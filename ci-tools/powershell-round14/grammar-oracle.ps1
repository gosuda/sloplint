param([Parameter(Mandatory = $true)][string]$CasesPath)
$ErrorActionPreference = "Stop"
if ($PSVersionTable.PSVersion.ToString() -ne "7.7.0") { throw "Unexpected PowerShell version" }
$cases = Get-Content -Raw -LiteralPath $CasesPath | ConvertFrom-Json
foreach ($case in $cases) {
    $tokens = $null
    $errors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseInput($case.source, [ref]$tokens, [ref]$errors)
    $accepted = $errors.Count -eq 0
    [ordered]@{id=$case.id; accepted=$accepted; errors=@($errors | ForEach-Object { $_.ErrorId })} | ConvertTo-Json -Compress
    if ($accepted -ne $case.accepted) { throw "Parser result differs for $($case.id)" }
}
Write-Output "POWERSHELL_ORACLE_COMPLETE"
