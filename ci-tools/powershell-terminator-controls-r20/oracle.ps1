param([Parameter(Mandatory=$true)][string]$CasesPath)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.ToString() -ne "7.7.0") { throw "Unexpected PowerShell version" }
$cases = Get-Content -Raw -LiteralPath $CasesPath | ConvertFrom-Json
foreach ($case in $cases) {
    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseInput($case.source, [ref]$tokens, [ref]$errors)
    $record = [ordered]@{
        id = $case.id
        source = $case.source
        accepted = $errors.Count -eq 0
        errors = @($errors | ForEach-Object { [ordered]@{id=$_.ErrorId; text=$_.Extent.Text; start=$_.Extent.StartOffset; end=$_.Extent.EndOffset} })
        tokens = @($tokens | ForEach-Object { [ordered]@{kind=$_.Kind.ToString(); text=$_.Text; flags=$_.TokenFlags.ToString(); start=$_.Extent.StartOffset; end=$_.Extent.EndOffset} })
        branches = @($ast.FindAll({param($node) $node -is [System.Management.Automation.Language.IfStatementAst]}, $true) | ForEach-Object { [ordered]@{clauses=$_.Clauses.Count; has_else=$null -ne $_.ElseClause} })
        commands = @($ast.FindAll({param($node) $node -is [System.Management.Automation.Language.CommandAst]}, $true) | ForEach-Object { [ordered]@{name=$_.GetCommandName(); text=$_.Extent.Text; count=$_.CommandElements.Count-1} })
    }
    Write-Output ("TOKEN_CASE " + ($record | ConvertTo-Json -Depth 10 -Compress))
}
Write-Output "TOKEN_CASES_COMPLETE $($cases.Count)"
