#Requires -Version 5.1
[CmdletBinding()]
param()

$ErrorActionPreference = 'Stop'
$bootstrap = Join-Path (Split-Path -Parent $PSScriptRoot) 'install.ps1'
$bootstrapText = [IO.File]::ReadAllText($bootstrap)

function Assert-InlineDownloadPath {
    param([string]$InstallerText)

    # No installer arguments: reproduce the documented irm ... | iex entry point.
    # Stop at the first mocked request, before any download, install or PATH change.
    $requests = [Collections.Generic.List[string]]::new()
    function Invoke-RestMethod($Uri, $Headers, $TimeoutSec) {
        $requests.Add([string]$Uri)
        throw 'KShell-Inline-Test-Release-Lookup'
    }
    $failure = $null
    $originalProgressPreference = $ProgressPreference
    try {
        $InstallerText | Invoke-Expression
    } catch {
        $failure = $_
    }
    if ($requests.Count -ne 1 -or $requests[0] -cne 'https://api.github.com/repos/chaovanthoung168/k-shell/releases/latest') {
        throw "Inline installer did not enter online mode. Error: $failure"
    }
    if ($null -eq $failure -or -not $failure.Exception.Message.Contains('KShell-Inline-Test-Release-Lookup')) {
        throw "Inline installer did not preserve the mocked release lookup failure. Error: $failure"
    }
    if ($ProgressPreference -ne $originalProgressPreference) { throw 'Installer did not restore native progress preferences after failure.' }
}

Assert-InlineDownloadPath $bootstrapText
foreach ($width in @(0, 1, 8, 24, 80, 120)) {
    $frame = [KShellSetupDisplay]::Frame(0, $width, 'Downloading a release', $false)
    if ($frame.Length -gt [Math]::Max(0, $width - 1)) { throw 'Pixel progress exceeds terminal width.' }
    if ($frame.Contains('%') -or $frame.Contains([string][char]27)) { throw 'Pixel frame includes a fabricated percentage or ANSI control.' }
}
$first = [KShellSetupDisplay]::Frame(0, 80, 'Downloading', $false).IndexOf([char]0x2588)
for ($tick = 0; $tick -lt 24; $tick++) {
    $head = [KShellSetupDisplay]::Frame($tick, 80, 'Downloading', $false).IndexOf([char]0x2588)
    if ($head -ne $first + $tick) { throw 'Pixel indicator does not move left to right.' }
}
if ([KShellSetupDisplay]::Frame(24, 80, 'Downloading', $false).IndexOf([char]0x2588) -ne $first) { throw 'Pixel indicator does not wrap to the left.' }
if (-not [Console]::IsOutputRedirected -and -not [Console]::IsErrorRedirected) {
    $cursorBefore = [Console]::CursorVisible
    $display = [KShellSetupDisplay]::new($false)
    try { $display.Update('Testing pixel progress'); Start-Sleep -Milliseconds 140; $display.Complete() }
    finally { $display.Dispose() }
    if ([Console]::CursorVisible -ne $cursorBefore) { throw 'Pixel display did not restore cursor visibility.' }
}
Write-Output 'Inline installer check passed: no-argument Invoke-Expression selects the online download path without installing or changing PATH.'
Write-Output 'Pixel progress checks passed: rightward frames, wrap, narrow widths, no fake percentages, and preference cleanup.'
