param([Parameter(Mandatory=$true)][string]$CasesPath)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.ToString() -ne '7.7.0') { throw 'Unexpected PowerShell version' }
function Write-Host { param([string]$Text) return $Text }
function Get-Date { return '__DATE__' }
$r = '__ROOT__'
$VERSION = '__VERSION__'
$manifest = @{ PrivateData = @{ PSData = @{ Prerelease = '__PRE__' } } }
$long_root_name = '__ROOT__'
Set-Variable -Name 'r/part' -Value '__ROOT__'
$env:TEMP = '__ROOT__'
$id = '__ID__'
$middle = '__MID__'
$π = '__PI__'
$foo = '__FOO__'
Set-Variable -Name '٢' -Value '__DIGIT__'
$cases = Get-Content -LiteralPath $CasesPath -Raw | ConvertFrom-Json
foreach ($case in $cases) {
    $tokens = $null
    $errors = $null
    $null = [System.Management.Automation.Language.Parser]::ParseInput($case.source, [ref]$tokens, [ref]$errors)
    if ($errors.Count -ne 0) { throw "Parse errors for $($case.name): $errors" }
    $actual = & ([scriptblock]::Create($case.source))
    if ($actual -cne $case.expected) {
        throw "Value mismatch for $($case.name): <$actual>, expected <$($case.expected)>"
    }
    Write-Output ('LITERAL_ORACLE ' + (@{ name = $case.name; value = $actual } | ConvertTo-Json -Compress))
}
Write-Output "LITERAL_ORACLE_COMPLETE $($cases.Count)"
