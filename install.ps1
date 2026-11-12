#Requires -Version 5.1
[CmdletBinding(DefaultParameterSetName = 'Online')]
param(
    [Parameter(ParameterSetName = 'Online')]
    [ValidatePattern('^(latest|v?[0-9]+\.[0-9]+\.[0-9]+)$')]
    [string]$Version = 'latest',
    [Parameter(Mandatory = $true, ParameterSetName = 'Local')]
    [string]$Archive,
    [Parameter(ParameterSetName = 'Local')]
    [string]$ChecksumFile,
    [string]$InstallDirectory,
    [string]$TerminalProfilePath,
    [string]$FontFace = 'auto',
    [ValidateSet('User', 'Process', 'None')]
    [string]$PathScope = 'User',
    [switch]$NoTerminalProfile
)

$ErrorActionPreference = 'Stop'
if ([Environment]::OSVersion.Platform -ne 'Win32NT' -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'K Shell requires 64-bit Windows 10 or 11.'
}
$repository = 'chaovanthoung168/k-shell'
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ('kshell-download-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($temporaryRoot) | Out-Null
$previousTls = [Net.ServicePointManager]::SecurityProtocol
try {
    [Net.ServicePointManager]::SecurityProtocol = $previousTls -bor [Net.SecurityProtocolType]::Tls12
    if ($PSCmdlet.ParameterSetName -eq 'Online') {
        $endpoint = 'latest'
        if ($Version -ne 'latest') { $endpoint = 'tags/v' + $Version.TrimStart('v') }
        $headers = @{ 'User-Agent' = 'KShell-Installer'; 'Accept' = 'application/vnd.github+json' }
        Write-Host 'Looking up the K Shell Windows release...'
        try {
            $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/$endpoint" -Headers $headers -TimeoutSec 60
        } catch {
            throw "Cannot find a published K Shell release. Check https://github.com/$repository/releases and your connection. $($_.Exception.Message)"
        }
        if ($release.draft -or $release.prerelease -or $release.tag_name -notmatch '^v([0-9]+\.[0-9]+\.[0-9]+)$') { throw 'Expected a stable vMAJOR.MINOR.PATCH release.' }
        $releaseVersion = $Matches[1]
        if ($Version -ne 'latest' -and $releaseVersion -ne $Version.TrimStart('v')) { throw 'GitHub returned a different release version.' }
        $archiveName = "KShell-$releaseVersion-windows-x64.zip"
        foreach ($name in @($archiveName, ($archiveName + '.sha256'))) {
            $asset = @($release.assets | Where-Object { $_.name -ceq $name })
            $expectedUrl = "https://github.com/$repository/releases/download/v$releaseVersion/$name"
            if ($asset.Count -ne 1 -or $asset[0].browser_download_url -cne $expectedUrl) { throw "Release is missing the expected asset: $name" }
            $destination = Join-Path $temporaryRoot $name
            Invoke-WebRequest -Uri $expectedUrl -Headers $headers -UseBasicParsing -OutFile $destination -TimeoutSec 180 | Out-Null
        }
        $Archive = Join-Path $temporaryRoot $archiveName
        $ChecksumFile = $Archive + '.sha256'
    } else {
        $Archive = (Resolve-Path -LiteralPath $Archive).ProviderPath
        $archiveName = [IO.Path]::GetFileName($Archive)
        if ($archiveName -notmatch '^KShell-([0-9]+\.[0-9]+\.[0-9]+)-windows-x64\.zip$') { throw 'Expected a KShell-MAJOR.MINOR.PATCH-windows-x64.zip archive.' }
        $releaseVersion = $Matches[1]
        if (-not $ChecksumFile) { $ChecksumFile = $Archive + '.sha256' }
    }
    if ((Get-Item -LiteralPath $Archive).Length -gt 100MB) { throw 'Release archive exceeds the supported size.' }
    $checksum = [IO.File]::ReadAllText((Resolve-Path -LiteralPath $ChecksumFile).ProviderPath).Trim()
    if ($checksum -notmatch '^([a-fA-F0-9]{64})\s+\*?([^\r\n]+)$' -or $Matches[2] -cne $archiveName) { throw 'Invalid checksum file or archive filename mismatch.' }
    $expectedHash = $Matches[1]
    if ((Get-FileHash -LiteralPath $Archive -Algorithm SHA256).Hash -ine $expectedHash) { throw 'ZIP checksum verification failed. Nothing has been installed.' }
    Write-Host "Verified K Shell $releaseVersion. Installing..."

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $extracted = Join-Path $temporaryRoot 'extracted'
    $packageName = "KShell-$releaseVersion-windows-x64"
    $zip = [IO.Compression.ZipFile]::OpenRead($Archive)
    try {
        if ($zip.Entries.Count -gt 2000) { throw 'Release archive has too many entries.' }
        $totalSize = 0L
        $seen = @{}
        foreach ($entry in $zip.Entries) {
            $name = $entry.FullName.Replace('\', '/')
            $parts = $name.TrimEnd('/').Split('/')
            if ($parts[0] -cne $packageName -or ($parts.Count -lt 2 -and -not $name.EndsWith('/'))) { throw 'Unexpected package root in release archive.' }
            foreach ($part in $parts) {
                if (-not $part -or $part -in @('.', '..') -or $part -match '[<>:"|?*\x00-\x1f]' -or $part -match '[. ]$' -or $part -match '^(?i:CON|PRN|AUX|NUL|COM[1-9]|LPT[1-9])(?:\.|$)') { throw 'Unsafe path in release archive.' }
            }
            if ($seen.ContainsKey($name.TrimEnd('/')) -or (($entry.ExternalAttributes -shr 16) -band 0xF000) -eq 0xA000 -or ($entry.ExternalAttributes -band 0x400)) { throw 'Duplicate or linked archive entry is unsupported.' }
            $seen[$name.TrimEnd('/')] = $true
            $totalSize += $entry.Length
            if ($totalSize -gt 256MB) { throw 'Extracted release exceeds the supported size.' }
        }
    } finally { $zip.Dispose() }
    [IO.Compression.ZipFile]::ExtractToDirectory($Archive, $extracted)
    $package = Join-Path $extracted $packageName
    $manifest = Get-Content -LiteralPath (Join-Path $package 'package-manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.version -cne $releaseVersion) { throw 'Package version does not match the release.' }
    $arguments = @{ FontFace = $FontFace; PathScope = $PathScope; NoTerminalProfile = $NoTerminalProfile }
    if ($InstallDirectory) { $arguments.InstallDirectory = $InstallDirectory }
    if ($TerminalProfilePath) { $arguments.TerminalProfilePath = $TerminalProfilePath }
    & (Join-Path $package 'Install.ps1') @arguments
    if ($PathScope -eq 'User') { Write-Host 'K Shell is ready. Restart your terminal app, then run kshell.' }
    elseif ($PathScope -eq 'Process') { Write-Host 'K Shell is ready. Run kshell in this PowerShell process.' }
    else { Write-Host 'K Shell is ready. Launch kshell.exe from your installation folder.' }
} finally {
    [Net.ServicePointManager]::SecurityProtocol = $previousTls
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $cleanup = [IO.Path]::GetFullPath($temporaryRoot)
    if (-not $cleanup.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($cleanup) -notmatch '^kshell-download-[0-9a-f]{32}$') { throw 'Refusing unsafe download cleanup.' }
    Remove-Item -LiteralPath $cleanup -Recurse -Force
}
