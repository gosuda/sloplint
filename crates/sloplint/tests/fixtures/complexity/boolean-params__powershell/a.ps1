function Confirm-Deploy {
    param(
        [bool]$Force,
        [bool]$DryRun
    )
    if ($Force) { "force" }
    if ($DryRun) { "dry" }
}
