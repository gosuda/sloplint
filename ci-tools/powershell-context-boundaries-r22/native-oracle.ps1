param([Parameter(Mandatory=$true)][string]$CasesPath)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.ToString() -ne "7.7.0") { throw "Unexpected PowerShell version" }
$cases = Get-Content -Raw -LiteralPath $CasesPath | ConvertFrom-Json
$mismatches = 0
foreach ($case in $cases) {
    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseInput($case.source, [ref]$tokens, [ref]$errors)
    $accepted = $errors.Count -eq 0
    $commands = @($ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.CommandAst] }, $true))
    $branches = @($ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.IfStatementAst] }, $true))
    $assignments = @($ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.AssignmentStatementAst] }, $true))
    $record = [ordered]@{
        id=$case.id
        kind=$case.kind
        accepted=$accepted
        source_sha256=$case.source_sha256
        errors=@($errors | ForEach-Object { $_.ErrorId })
        commands=@($commands | ForEach-Object { [ordered]@{name=$_.GetCommandName(); text=$_.Extent.Text; count=$_.CommandElements.Count-1; arguments=@($_.CommandElements | Select-Object -Skip 1 | ForEach-Object { [ordered]@{kind=$_.GetType().Name; text=$_.Extent.Text; start=$_.Extent.StartOffset; end=$_.Extent.EndOffset} })} })
        branches=@($branches | ForEach-Object { [ordered]@{clauses=$_.Clauses.Count; has_else=$null -ne $_.ElseClause; text=$_.Extent.Text} })
        redirections=@($ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.FileRedirectionAst] }, $true) | ForEach-Object { [ordered]@{file=$_.Location.Extent.Text; text=$_.Extent.Text; start=$_.Extent.StartOffset; end=$_.Extent.EndOffset} })
        assignments=@($assignments | ForEach-Object { [ordered]@{right_kind=$_.Right.GetType().Name; right_text=$_.Right.Extent.Text} })
        tokens=@($tokens | ForEach-Object { [ordered]@{kind=$_.Kind.ToString(); text=$_.Text; flags=$_.TokenFlags.ToString(); start=$_.Extent.StartOffset; end=$_.Extent.EndOffset} })
        matches=$null -eq $case.accepted -or $accepted -eq $case.accepted
    }
    if ($accepted -and $case.kind -eq "arguments") {
        try {
            $json = & ([scriptblock]::Create($case.source))
            if ($LASTEXITCODE -ne 0) { throw "Native argv capture failed: $LASTEXITCODE" }
            $native = $json | ConvertFrom-Json
            $record["native_count"] = $native.count
            $record["native_values"] = @($native.values)
            if ($commands.Count -ne 1) { throw "Expected one source command" }
            $arguments = @($commands[0].CommandElements | Select-Object -Skip 1)
            $record["argument_nodes"] = @($arguments | ForEach-Object { [ordered]@{kind=$_.GetType().Name; text=$_.Extent.Text; start=$_.Extent.StartOffset; end=$_.Extent.EndOffset} })
            if ($case.kind -eq "arguments") {
                $record.matches = $record.matches -and $native.count -eq $case.expected_values.Count -and $arguments.Count -eq $case.argument_sources.Count
                for ($index=0; $index -lt $case.expected_values.Count; $index++) {
                    if ($index -ge $native.values.Count -or $native.values[$index] -cne $case.expected_values[$index]) { $record.matches=$false }
                    if ($index -ge $arguments.Count -or $arguments[$index].Extent.Text -cne $case.argument_sources[$index]) { $record.matches=$false }
                }
            }
        } catch {
            $record["runtime_error"]=$_.Exception.Message
            $record.matches=$false
        }
    }
    if (-not $record.matches) { $mismatches++ }
    Write-Output ("NATIVE_CONTEXT " + ($record | ConvertTo-Json -Depth 12 -Compress))
}
Write-Output ("NATIVE_CONTEXT_COMPLETE " + (@{count=$cases.Count; mismatches=$mismatches} | ConvertTo-Json -Compress))
if ($mismatches -ne 0) { exit 1 }
