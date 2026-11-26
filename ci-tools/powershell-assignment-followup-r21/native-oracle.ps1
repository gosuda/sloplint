param([Parameter(Mandatory = $true)][string]$CasesPath)
$ErrorActionPreference = "Stop"
Set-StrictMode -Version Latest
if ($PSVersionTable.PSVersion.ToString() -ne "7.7.0") { throw "Unexpected PowerShell version" }
function Write-Host {
    if ($args.Count -ne 1) { throw "Literal witness expected one argument, got $($args.Count)" }
    return [string]$args[0]
}
$cases = Get-Content -Raw -LiteralPath $CasesPath | ConvertFrom-Json
$mismatches = 0
foreach ($case in $cases) {
    $tokens = $null
    $errors = $null
    $ast = [System.Management.Automation.Language.Parser]::ParseInput($case.source, [ref]$tokens, [ref]$errors)
    $accepted = $errors.Count -eq 0
    $record = [ordered]@{
        id = $case.id
        kind = $case.kind
        accepted = $accepted
        errors = @($errors | ForEach-Object { $_.ErrorId })
        source_sha256 = $case.source_sha256
        matches = $accepted -eq $case.accepted
    }
    try {
        if ($accepted -and $case.kind -eq "arguments") {
            $json = & ([scriptblock]::Create($case.source))
            if ($LASTEXITCODE -ne 0) { throw "Native argv capture failed: $LASTEXITCODE" }
            $native = $json | ConvertFrom-Json
            $record["native_count"] = $native.count
            $record["native_values"] = @($native.values)
            $commands = @($ast.FindAll({ param($node) $node -is [System.Management.Automation.Language.CommandAst] }, $true))
            if ($commands.Count -ne 1) { throw "Expected one source command" }
            $record["ast_argument_count"] = $commands[0].CommandElements.Count - 1
            $record["matches"] = $record["matches"] -and $native.count -eq $case.expected_values.Count
            for ($index = 0; $index -lt $case.expected_values.Count; $index++) {
                if ($index -ge $native.values.Count -or $native.values[$index] -cne $case.expected_values[$index]) {
                    $record["matches"] = $false
                }
            }
        } elseif ($accepted -and $case.kind -eq "literal") {
            $actual = [string](& ([scriptblock]::Create($case.source)))
            $record["value"] = $actual
            $record["matches"] = $record["matches"] -and $actual -ceq $case.expected_value
        }
    } catch {
        $record["runtime_error"] = $_.Exception.Message
        $record["matches"] = $false
    }
    if (-not $record["matches"]) { $mismatches++ }
    Write-Output ("NATIVE_CASE " + ($record | ConvertTo-Json -Depth 8 -Compress))
}
Write-Output ("NATIVE_CASES_COMPLETE " + (@{count=$cases.Count; mismatches=$mismatches} | ConvertTo-Json -Compress))
if ($mismatches -ne 0) { exit 1 }
