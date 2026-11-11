function Confirm-Deploy {
    param(
        [bool]$Force,
        [string]$Target
    )
    if ($Force) { "force" }
    $Target
}
