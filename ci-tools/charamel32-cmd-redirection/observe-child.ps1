$ErrorActionPreference = 'Stop'
[Console]::OutputEncoding = New-Object System.Text.UTF8Encoding($false)
$selfProcess = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $PID" -OperationTimeoutSec 5
if ($null -eq $selfProcess) { throw 'Owned observer process was not found' }
$parentId = [uint32]$selfProcess.ParentProcessId
$parentProcess = Get-CimInstance -ClassName Win32_Process -Filter "ProcessId = $parentId" -OperationTimeoutSec 5
if ($null -eq $parentProcess -or $parentProcess.Name -ine 'cmd.exe') {
    throw 'The immediate parent of the owned observer is not Cmd'
}
[ordered]@{
    observer_pid = $PID
    parent_pid = $parentId
    parent_name = $parentProcess.Name
    parent_command_line = $parentProcess.CommandLine
    arguments = @($args)
} | ConvertTo-Json -Compress -Depth 4
