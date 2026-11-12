[CmdletBinding()]
param(
    [string]$InstallDirectory,
    [string]$TerminalProfilePath,
    [string]$FontFace = 'auto',
    [switch]$NoTerminalProfile
)

$ErrorActionPreference = 'Stop'
$sourceExecutable = Join-Path $PSScriptRoot 'kshell.exe'
foreach ($requiredFile in @('kshell.exe','package-manifest.json','Install-KShellProfile.ps1','LICENSE','THIRD-PARTY-NOTICES.txt','README.txt')) {
    if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot $requiredFile) -PathType Leaf)) { throw "Incomplete package: $requiredFile" }
}
if (-not (Test-Path -LiteralPath (Join-Path $PSScriptRoot 'licenses') -PathType Container)) { throw 'Incomplete package: dependency licenses.' }
$manifest = Get-Content -LiteralPath (Join-Path $PSScriptRoot 'package-manifest.json') -Raw | ConvertFrom-Json
if ($manifest.application -ne 'K Shell' -or $manifest.platform -ne 'windows-x64') { throw 'Unsupported K Shell package.' }
if ((Get-FileHash -LiteralPath $sourceExecutable -Algorithm SHA256).Hash -ne $manifest.sha256) { throw 'Package integrity check failed: kshell.exe does not match its manifest.' }
if (-not [Environment]::Is64BitOperatingSystem) { throw 'This package requires 64-bit Windows.' }
if (-not $InstallDirectory) { $InstallDirectory = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\KShell' }
$installPath = [IO.Path]::GetFullPath($InstallDirectory).TrimEnd('\','/')
if ($installPath -eq [IO.Path]::GetPathRoot($installPath).TrimEnd('\','/') -or $installPath -eq [Environment]::GetFolderPath('UserProfile')) { throw 'Choose a dedicated K Shell installation folder.' }
$installedExecutable = Join-Path $installPath 'kshell.exe'
if ([IO.Path]::GetFullPath($sourceExecutable) -eq [IO.Path]::GetFullPath($installedExecutable)) { throw 'For portable use, run kshell.exe directly; choose another folder to install.' }
$markerPath = Join-Path $installPath 'kshell-install.json'
if (Test-Path -LiteralPath $installPath) {
    $directory = Get-Item -LiteralPath $installPath
    if ($directory.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Installation through a link/reparse point is unsupported.' }
    if (Test-Path -LiteralPath $markerPath) {
        $marker = Get-Content -LiteralPath $markerPath -Raw | ConvertFrom-Json
        if ($marker.application -ne 'K Shell') { throw 'Installation marker belongs to another application.' }
    } elseif (@(Get-ChildItem -LiteralPath $installPath -Force).Count -gt 0) {
        throw 'Refusing to overwrite an existing, nonempty unmarked folder.'
    }
}
$ancestor = [IO.Directory]::GetParent($installPath)
while ($ancestor) {
    if ($ancestor.Exists -and ($ancestor.Attributes -band [IO.FileAttributes]::ReparsePoint)) { throw 'Installation through a link/reparse ancestor is unsupported.' }
    $ancestor = $ancestor.Parent
}
[IO.Directory]::CreateDirectory($installPath) | Out-Null
$stagedExecutable = Join-Path $installPath ('kshell-staged-' + [Guid]::NewGuid().ToString('N') + '.exe')
Copy-Item -LiteralPath $sourceExecutable -Destination $stagedExecutable -ErrorAction Stop
$backup = $null
try {
    if (Test-Path -LiteralPath $installedExecutable) {
        $backup = Join-Path $installPath ('kshell-previous-' + [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff') + '.exe')
        [IO.File]::Move($installedExecutable, $backup)
    }
    [IO.File]::Move($stagedExecutable, $installedExecutable)
} catch {
    if ($backup -and -not (Test-Path -LiteralPath $installedExecutable)) { [IO.File]::Move($backup, $installedExecutable) }
    throw
}
$record = [ordered]@{ application = 'K Shell'; version = $manifest.version; executable = $installedExecutable }
[IO.File]::WriteAllText($markerPath, ($record | ConvertTo-Json), [Text.UTF8Encoding]::new($false))
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'Install-KShellProfile.ps1') -Destination (Join-Path $installPath 'Install-KShellProfile.ps1') -Force
foreach ($document in @('LICENSE','THIRD-PARTY-NOTICES.txt','README.txt')) {
    Copy-Item -LiteralPath (Join-Path $PSScriptRoot $document) -Destination (Join-Path $installPath $document) -Force
}
Copy-Item -LiteralPath (Join-Path $PSScriptRoot 'licenses') -Destination $installPath -Recurse -Force
if (-not $NoTerminalProfile) {
    $profileArguments = @{ Executable = $installedExecutable; FontFace = $FontFace }
    if ($TerminalProfilePath) { $profileArguments.OutputPath = $TerminalProfilePath }
    & (Join-Path $installPath 'Install-KShellProfile.ps1') @profileArguments
}
Write-Output "Installed K Shell $($manifest.version): $installedExecutable"
if (-not $NoTerminalProfile) { Write-Output 'Close and reopen Windows Terminal, then select K Shell from the profile dropdown.' }
Write-Output 'Portable/direct launch also works by running the installed kshell.exe.'
