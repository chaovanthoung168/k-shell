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
}

Assert-InlineDownloadPath $bootstrapText
Write-Output 'Inline installer check passed: no-argument Invoke-Expression selects the online download path without installing or changing PATH.'
