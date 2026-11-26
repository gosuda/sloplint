param([Parameter(Mandatory=$true)][string]$CasesPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.ToString() -ne '7.7.0') { throw 'Unexpected PowerShell version' }
$cases = Get-Content -LiteralPath $CasesPath -Raw | ConvertFrom-Json
foreach ($case in $cases) {
    $tokens = $null
    $errors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseInput($case.source, [ref]$tokens, [ref]$errors)
    Write-Output ('NATIVE_GRAMMAR ' + (@{ id = $case.id; accepted = $errors.Count -eq 0; errors = @($errors | ForEach-Object ErrorId) } | ConvertTo-Json -Compress))
}
Write-Output "NATIVE_GRAMMAR_COMPLETE $($cases.Count)"
