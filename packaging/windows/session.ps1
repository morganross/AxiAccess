# AxiAccess's per-user lifecycle only. No service, daemon or provider changes.
[CmdletBinding()]
param(
    [Parameter(Mandatory)][ValidateSet('Start','Stop','Status','EnableStartup','DisableStartup')][string]$Operation,
    [string]$ConfigPath = (Join-Path $env:LOCALAPPDATA 'AxiAccess\sender.json')
)
$ErrorActionPreference = 'Stop'
$image = [IO.Path]::GetFullPath((Join-Path $PSScriptRoot 'axiaccess.exe'))
$sid = [Security.Principal.WindowsIdentity]::GetCurrent().User.Value
$currentSession = (Get-Process -Id $PID).SessionId
$ConfigPath=[IO.Path]::GetFullPath($ConfigPath)
$startupKey='HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'
if($Operation -eq 'EnableStartup'){
    $powershell=Join-Path $env:SystemRoot 'System32\WindowsPowerShell\v1.0\powershell.exe'
    $startup='"'+$powershell+'" -NoProfile -NonInteractive -WindowStyle Hidden -ExecutionPolicy Bypass -File "'+$PSCommandPath+'" -Operation Start -ConfigPath "'+$ConfigPath+'"'
    New-Item -Path $startupKey -Force|Out-Null
    New-ItemProperty -Path $startupKey -Name 'AxiAccess Sender' -Value $startup -PropertyType String -Force|Out-Null
    exit 0
}
if($Operation -eq 'DisableStartup'){
    Remove-ItemProperty -Path $startupKey -Name 'AxiAccess Sender' -ErrorAction SilentlyContinue
    exit 0
}
$owned = @()
foreach ($candidate in @(Get-CimInstance Win32_Process -Filter "Name='axiaccess.exe'")) {
    if (!$candidate.ExecutablePath -or ![IO.Path]::GetFullPath($candidate.ExecutablePath).Equals($image,[StringComparison]::OrdinalIgnoreCase)) { continue }
    $owner = Invoke-CimMethod -InputObject $candidate -MethodName GetOwnerSid
    if ($owner.ReturnValue -eq 0 -and $owner.Sid -eq $sid -and $candidate.CommandLine -match '(?i)(?:^|\s)sender(?:\s|$)') { $owned += $candidate }
}
if ($Operation -eq 'Stop') {
    foreach ($candidate in $owned) {
        # Re-read both identity and path immediately before touching a PID.
        $live = Get-CimInstance Win32_Process -Filter ('ProcessId='+$candidate.ProcessId) -ErrorAction SilentlyContinue
        if (!$live -or $live.CreationDate -ne $candidate.CreationDate -or $live.ExecutablePath -ine $image) { continue }
        $owner = Invoke-CimMethod -InputObject $live -MethodName GetOwnerSid
        if ($owner.ReturnValue -ne 0 -or $owner.Sid -ne $sid) { throw 'AxiAccess process ownership changed during shutdown.' }
        $result = Invoke-CimMethod -InputObject $live -MethodName Terminate -Arguments @{Reason=[uint32]0}
        if ($result.ReturnValue -ne 0) { throw ('Unable to stop this user''s installed AxiAccess sender: '+$result.ReturnValue) }
        $process = Get-Process -Id $candidate.ProcessId -ErrorAction SilentlyContinue
        if ($process -and !$process.WaitForExit(10000)) { throw 'AxiAccess sender did not exit before file replacement.' }
    }
    exit 0
}
if ($Operation -eq 'Status') {
    $startup=Get-ItemProperty -Path $startupKey -Name 'AxiAccess Sender' -ErrorAction SilentlyContinue
    [pscustomobject]@{running=($owned.Count -gt 0);processIds=@($owned.ProcessId);image=$image;config=$ConfigPath;startupEnabled=($null -ne $startup)} | ConvertTo-Json -Depth 3
    exit 0
}
if ($currentSession -eq 0) {
    # SSH/MSI can configure this account without having its selected graphical
    # desktop. Do not create a misleading Session0 observer or guess an RDP user.
    [pscustomobject]@{startDeferred=$true;reason='Run Start in the selected graphical user session, or sign in to use the registered logon startup.'}|ConvertTo-Json
    exit 0
}
if (@($owned|Where-Object SessionId -eq $currentSession).Count) { exit 0 }
if (!(Test-Path -LiteralPath $ConfigPath -PathType Leaf)) { throw 'AxiAccess sender configuration is missing; run axiaccess setup first.' }
$child = Start-Process -FilePath $image -ArgumentList @('sender','--config',('"'+$ConfigPath+'"')) -WorkingDirectory $PSScriptRoot -WindowStyle Hidden -PassThru
if ($child.WaitForExit(750)) { throw ('AxiAccess sender exited during startup: '+$child.ExitCode) }
exit 0
