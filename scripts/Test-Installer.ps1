#Requires -Version 5.1
[CmdletBinding()]
param([Parameter(Mandatory = $true)][string]$Archive, [switch]$LiveRelease)

$ErrorActionPreference = 'Stop'
$Archive = (Resolve-Path -LiteralPath $Archive).ProviderPath
$bootstrap = Join-Path (Split-Path -Parent $PSScriptRoot) 'install.ps1'
$testRoot = Join-Path ([IO.Path]::GetTempPath()) ('kshell-installer-test-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($testRoot) | Out-Null
$previousPath = $env:PATH
$previousUserPath = [Environment]::GetEnvironmentVariable('Path', 'User')
function Assert-True($Condition, [string]$Message) { if (-not $Condition) { throw $Message } }
function Assert-Rejected([scriptblock]$Action, [string]$Message) {
    $rejected = $false
    try { & $Action } catch { $rejected = $true; Write-Host "Rejected as expected: $($_.Exception.Message)" }
    Assert-True $rejected $Message
}
try {
    $install = Join-Path $testRoot 'installed shell with spaces'
    $profile = Join-Path $testRoot 'private terminal\kshell.json'
    & $bootstrap -Archive $Archive -InstallDirectory $install -TerminalProfilePath $profile -FontFace Consolas -PathScope Process
    $executable = Join-Path $install 'kshell.exe'
    Assert-True (Test-Path -LiteralPath $profile) 'Bootstrap did not create the requested private Terminal profile.'
    Assert-True ((Get-Command kshell.exe).Source -eq $executable) 'Installed shell cannot be found on process PATH.'
    $help = & $executable --help
    Assert-True ($LASTEXITCODE -eq 0 -and ($help -join "`n").Contains('K Shell')) 'Installed shell did not run.'
    & $bootstrap -Archive $Archive -InstallDirectory $install -NoTerminalProfile -PathScope Process
    Assert-True (@($env:PATH -split ';' | Where-Object { $_ -ieq $install }).Count -eq 1) 'Upgrade duplicated the PATH entry.'
    Assert-True (@(Get-ChildItem -LiteralPath $install -Filter 'kshell-previous-*.exe').Count -eq 1) 'Bootstrap upgrade did not preserve the previous executable.'

    $archiveName = [IO.Path]::GetFileName($Archive)
    $version = [regex]::Match($archiveName, '^KShell-([0-9]+\.[0-9]+\.[0-9]+)-windows-x64\.zip$').Groups[1].Value
    $url = "https://github.com/chaovanthoung168/k-shell/releases/download/v$version/"
    $apiCalls = [Collections.Generic.List[string]]::new()
    $downloads = [Collections.Generic.List[string]]::new()
    $invalidAssets = $false
    $fixtureArchive = $Archive
    $fixtureArchiveName = $archiveName
    $fixtureVersion = $version
    $fixtureUrl = $url
    if (-not $LiveRelease) {
        # Local fixtures exercise the real online selection/download path without network or credentials.
        function Invoke-RestMethod($Uri, $Headers, $TimeoutSec) {
            Assert-True ($Uri -in @('https://api.github.com/repos/chaovanthoung168/k-shell/releases/latest', "https://api.github.com/repos/chaovanthoung168/k-shell/releases/tags/v$fixtureVersion")) 'Unexpected release endpoint.'
            $apiCalls.Add($Uri)
            $assets = @(@{ name=$fixtureArchiveName; browser_download_url=($fixtureUrl + $fixtureArchiveName) }, @{ name=($fixtureArchiveName + '.sha256'); browser_download_url=($fixtureUrl + $fixtureArchiveName + '.sha256') })
            if ($invalidAssets) { $assets = @() }
            return @{ tag_name="v$fixtureVersion"; draft=$false; prerelease=$false; assets=$assets }
        }
        function Invoke-WebRequest($Uri, $Headers, [switch]$UseBasicParsing, $OutFile, $TimeoutSec) {
            Assert-True $UseBasicParsing 'Windows PowerShell downloads must use basic parsing.'
            Assert-True ($Uri -in @(($fixtureUrl + $fixtureArchiveName), ($fixtureUrl + $fixtureArchiveName + '.sha256'))) 'Unexpected release download URL.'
            $downloads.Add($Uri)
            $fixture = $fixtureArchive
            if ($Uri.EndsWith('.sha256')) { $fixture += '.sha256' }
            Copy-Item -LiteralPath $fixture -Destination $OutFile
        }
    }
    $onlineInstall = Join-Path $testRoot 'online installation'
    $unusedProfile = Join-Path $testRoot 'must-not-write.json'
    & $bootstrap -InstallDirectory $onlineInstall -TerminalProfilePath $unusedProfile -NoTerminalProfile -PathScope None
    Assert-True (Test-Path -LiteralPath (Join-Path $onlineInstall 'kshell.exe')) 'Online installation failed.'
    Assert-True (-not (Test-Path -LiteralPath $unusedProfile)) 'NoTerminalProfile unexpectedly wrote a profile.'
    if (-not $LiveRelease) {
        & $bootstrap -Version $version -InstallDirectory $onlineInstall -NoTerminalProfile -PathScope None
        Assert-True ($apiCalls.Count -eq 2 -and $downloads.Count -eq 4) 'Online downloads or version selection were skipped.'
        $invalidAssets = $true
        Assert-Rejected { & $bootstrap -InstallDirectory (Join-Path $testRoot 'missing assets') -NoTerminalProfile -PathScope None } 'Missing assets were accepted.'
    }

    $badChecksum = Join-Path $testRoot 'bad.sha256'
    [IO.File]::WriteAllText($badChecksum, (('0' * 64) + '  ' + $archiveName))
    $invalidInstall = Join-Path $testRoot 'must not install'
    Assert-Rejected { & $bootstrap -Archive $Archive -ChecksumFile $badChecksum -InstallDirectory $invalidInstall -NoTerminalProfile -PathScope None } 'Invalid ZIP checksum was accepted.'
    Assert-True (-not (Test-Path -LiteralPath $invalidInstall)) 'Checksum failure wrote installation files.'

    $malicious = Join-Path $testRoot $archiveName
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open($malicious, 'Create')
    try { $zip.CreateEntry("KShell-$version-windows-x64/../escaped.txt") | Out-Null } finally { $zip.Dispose() }
    [IO.File]::WriteAllText(($malicious + '.sha256'), ((Get-FileHash -LiteralPath $malicious).Hash + '  ' + $archiveName))
    Assert-Rejected { & $bootstrap -Archive $malicious -InstallDirectory $invalidInstall -NoTerminalProfile -PathScope None } 'Archive traversal was accepted.'
    Assert-True (-not (Test-Path -LiteralPath $invalidInstall)) 'Unsafe archive wrote installation files.'
    Assert-True ([Environment]::GetEnvironmentVariable('Path', 'User') -ceq $previousUserPath) 'Tests modified the real user PATH.'
    Write-Output 'Installer checks passed: verified local/online install, version selection, executable, private profile, process PATH, idempotent upgrade, missing assets, checksum rejection and archive traversal rejection.'
} finally {
    $env:PATH = $previousPath
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $cleanup = [IO.Path]::GetFullPath($testRoot)
    if (-not $cleanup.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($cleanup) -notmatch '^kshell-installer-test-[0-9a-f]{32}$') { throw 'Refusing unsafe test cleanup.' }
    Remove-Item -LiteralPath $cleanup -Recurse -Force
}
