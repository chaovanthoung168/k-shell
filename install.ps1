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
    [switch]$NoTerminalProfile,
    [switch]$NoAnimation,
    [switch]$Launch
)

$ErrorActionPreference = 'Stop'
if (-not ('KShellSetupDisplay' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Threading;
public sealed class KShellSetupDisplay : IDisposable {
    private readonly object gate = new object();
    private readonly Thread worker;
    private readonly bool color;
    private bool previousCursor, ownsCursor, finished;
    private volatile bool running = true;
    private string stage = "Preparing setup";
    private int tick, lastLength;
    public KShellSetupDisplay(bool useColor) {
        color = useColor;
        try { previousCursor = Console.CursorVisible; Console.CursorVisible = false; ownsCursor = true; } catch { }
        worker = new Thread(Animate); worker.IsBackground = true; worker.Start();
    }
    public static string Frame(int frame, int width, string message, bool complete) {
        int size = Math.Max(1, Math.Min(24, width / 3));
        char[] pixels = new string('\u2591', size).ToCharArray();
        int head = Math.Max(0, frame) % size;
        for (int i = 0; i < size; i++) {
            if (complete || i == head) pixels[i] = '\u2588';
            else if (i == head - 1) pixels[i] = '\u2593';
            else if (i == head - 2) pixels[i] = '\u2592';
        }
        string line = "  K Shell [" + new string(pixels) + "] " + message;
        int limit = Math.Max(0, width - 1);
        return line.Length > limit ? line.Substring(0, limit) : line;
    }
    public void Update(string message) { lock (gate) { stage = message; } }
    private void Animate() {
        while (running) {
            try { lock (gate) { Draw(false); } } catch { running = false; }
            Thread.Sleep(33);
        }
    }
    private void Draw(bool complete) {
        int width = 80;
        try { width = Math.Max(1, Console.WindowWidth); } catch { }
        string line = Frame(tick++, width, stage, complete);
        ConsoleColor previous = Console.ForegroundColor;
        try {
            if (color) Console.ForegroundColor = ConsoleColor.Cyan;
            Console.Error.Write("\r" + line.PadRight(Math.Min(Math.Max(lastLength, line.Length), Math.Max(0, width - 1))));
            Console.Error.Flush(); lastLength = line.Length;
        } finally { if (color) Console.ForegroundColor = previous; }
    }
    public void Complete() { Finish(true); }
    public void Dispose() { Finish(false); }
    private void Finish(bool success) {
        if (finished) return;
        finished = true; running = false; worker.Join();
        try {
            lock (gate) { stage = success ? "Installation complete" : "Setup stopped"; Draw(success); Console.Error.WriteLine(); }
        } finally { if (ownsCursor) { try { Console.CursorVisible = previousCursor; } catch { } } }
    }
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr SendMessageTimeout(IntPtr window, uint message, UIntPtr parameter,
        string area, uint flags, uint timeout, out UIntPtr result);
    public static void NotifyEnvironmentChanged() {
        UIntPtr result;
        SendMessageTimeout(new IntPtr(0xffff), 0x001a, UIntPtr.Zero, "Environment", 2, 1000, out result);
    }
}
'@
}
$setupDisplay = $null
function Show-KShellProgress([int]$Percent, [string]$Stage) {
    # One indeterminate pixel moves left to right; percentages are not transfer data.
    if ($null -ne $setupDisplay) { $setupDisplay.Update($Stage) }
    else { Write-Host "K Shell: $Stage" }
}
if ([Environment]::OSVersion.Platform -ne 'Win32NT' -or -not [Environment]::Is64BitOperatingSystem) {
    throw 'K Shell requires 64-bit Windows 10 or 11.'
}
$repository = 'chaovanthoung168/k-shell'
$temporaryRoot = Join-Path ([IO.Path]::GetTempPath()) ('kshell-download-' + [Guid]::NewGuid().ToString('N'))
[IO.Directory]::CreateDirectory($temporaryRoot) | Out-Null
$previousTls = [Net.ServicePointManager]::SecurityProtocol
$previousProgressPreference = $ProgressPreference
try {
    $ProgressPreference = 'SilentlyContinue'
    if (-not $NoAnimation -and $Host.Name -eq 'ConsoleHost' -and -not [Console]::IsOutputRedirected -and -not [Console]::IsErrorRedirected) {
        $setupDisplay = [KShellSetupDisplay]::new(-not (Test-Path Env:NO_COLOR))
    }
    Show-KShellProgress 5 'Preparing setup'
    [Net.ServicePointManager]::SecurityProtocol = $previousTls -bor [Net.SecurityProtocolType]::Tls12
    # Invoke-Expression does not resolve this script's default parameter set.
    # The documented irm ... | iex form has no Archive and must download online.
    if ([string]::IsNullOrEmpty($Archive)) {
        $endpoint = 'latest'
        if ($Version -ne 'latest') { $endpoint = 'tags/v' + $Version.TrimStart('v') }
        $headers = @{ 'User-Agent' = 'KShell-Installer'; 'Accept' = 'application/vnd.github+json' }
        Show-KShellProgress 5 'Finding the latest release'
        try {
            $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repository/releases/$endpoint" -Headers $headers -TimeoutSec 60
        } catch {
            throw "Cannot find a published K Shell release. Check https://github.com/$repository/releases and your connection. $($_.Exception.Message)"
        }
        if ($release.draft -or $release.prerelease -or $release.tag_name -notmatch '^v([0-9]+\.[0-9]+\.[0-9]+)$') { throw 'Expected a stable vMAJOR.MINOR.PATCH release.' }
        $releaseVersion = $Matches[1]
        Show-KShellProgress 20 ("Found stable release v" + $releaseVersion)
        if ($Version -ne 'latest' -and $releaseVersion -ne $Version.TrimStart('v')) { throw 'GitHub returned a different release version.' }
        $archiveName = "KShell-$releaseVersion-windows-x64.zip"
        foreach ($name in @($archiveName, ($archiveName + '.sha256'))) {
            $asset = @($release.assets | Where-Object { $_.name -ceq $name })
            $expectedUrl = "https://github.com/$repository/releases/download/v$releaseVersion/$name"
            if ($asset.Count -ne 1 -or $asset[0].browser_download_url -cne $expectedUrl) { throw "Release is missing the expected asset: $name" }
            $destination = Join-Path $temporaryRoot $name
            Show-KShellProgress 20 ("Downloading " + $name)
            Invoke-WebRequest -Uri $expectedUrl -Headers $headers -UseBasicParsing -OutFile $destination -TimeoutSec 180 | Out-Null
        }
        $Archive = Join-Path $temporaryRoot $archiveName
        $ChecksumFile = $Archive + '.sha256'
        Show-KShellProgress 45 'Downloaded release'
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
    Show-KShellProgress 60 'Verified download checksum'

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
    Show-KShellProgress 75 'Extracted verified package'
    $package = Join-Path $extracted $packageName
    $manifest = Get-Content -LiteralPath (Join-Path $package 'package-manifest.json') -Raw | ConvertFrom-Json
    if ($manifest.version -cne $releaseVersion) { throw 'Package version does not match the release.' }
    $arguments = @{ FontFace = $FontFace; PathScope = $PathScope; NoTerminalProfile = $NoTerminalProfile }
    # Older public packages do not have the display switch yet.
    if ((Get-Command (Join-Path $package 'Install.ps1')).Parameters.ContainsKey('NoAnimation')) { $arguments.NoAnimation = $true }
    if ($InstallDirectory) { $arguments.InstallDirectory = $InstallDirectory }
    if ($TerminalProfilePath) { $arguments.TerminalProfilePath = $TerminalProfilePath }
    Show-KShellProgress 80 ("Installing K Shell v" + $releaseVersion)
    # Legacy packages also write their own progress and banner. Keep one owner of
    # terminal output, while retaining every installer stream for error reporting.
    $setupLog = Join-Path $temporaryRoot 'package-setup.log'
    try { & (Join-Path $package 'Install.ps1') @arguments *> $setupLog }
    catch {
        if ($null -ne $setupDisplay) { $setupDisplay.Dispose(); $setupDisplay = $null }
        if (Test-Path -LiteralPath $setupLog) { Get-Content -LiteralPath $setupLog -Tail 12 | Write-Host }
        throw
    }
    $installedFolder = $InstallDirectory
    if (-not $installedFolder) { $installedFolder = Join-Path ([Environment]::GetFolderPath('LocalApplicationData')) 'Programs\KShell' }
    $installedExecutable = Join-Path ([IO.Path]::GetFullPath($installedFolder)) 'kshell.exe'
    if ($PathScope -eq 'User') { [KShellSetupDisplay]::NotifyEnvironmentChanged() }
    if ($null -ne $setupDisplay) { $setupDisplay.Complete() }
    else { Show-KShellProgress 100 'Installation complete' }
    & $installedExecutable --welcome
    if ($LASTEXITCODE -ne 0) { throw 'K Shell was installed, but the welcome check failed.' }
    Write-Host ("Installed K Shell v" + $releaseVersion)
    $launchCommand = "& '" + $installedExecutable.Replace("'", "''") + "'"
    Write-Host 'Start K Shell now (works in your current terminal):'
    Write-Host "  $launchCommand"
    if ($PathScope -eq 'User') { Write-Host 'For the short kshell command, close ALL terminal windows and reopen the terminal app. A child installer cannot update its parent PowerShell PATH.' }
    elseif ($PathScope -eq 'Process') { Write-Host 'kshell is also available in this installer PowerShell process.' }
} finally {
    if ($null -ne $setupDisplay) { $setupDisplay.Dispose() }
    $ProgressPreference = $previousProgressPreference
    [Net.ServicePointManager]::SecurityProtocol = $previousTls
    $tempBase = [IO.Path]::GetFullPath([IO.Path]::GetTempPath()).TrimEnd('\') + '\'
    $cleanup = [IO.Path]::GetFullPath($temporaryRoot)
    if (-not $cleanup.StartsWith($tempBase, [StringComparison]::OrdinalIgnoreCase) -or [IO.Path]::GetFileName($cleanup) -notmatch '^kshell-download-[0-9a-f]{32}$') { throw 'Refusing unsafe download cleanup.' }
    Remove-Item -LiteralPath $cleanup -Recurse -Force
}
if ($Launch) { & $installedExecutable }
