[CmdletBinding()]
param(
    [string]$Executable,
    [string]$FontFace = 'auto',
    [ValidateSet('auto','unicode','nerd','off')][string]$Icons = 'auto',
    [string]$OutputPath,
    [switch]$Preview
)

$ErrorActionPreference = 'Stop'
if (-not $Executable) {
    $portableExecutable = Join-Path $PSScriptRoot 'kshell.exe'
    $Executable = if (Test-Path -LiteralPath $portableExecutable -PathType Leaf) { $portableExecutable } else { Join-Path $PSScriptRoot '..\..\target\debug\kshell.exe' }
}
$resolvedExecutable = (Resolve-Path -LiteralPath $Executable).ProviderPath
if (-not (Test-Path -LiteralPath $resolvedExecutable -PathType Leaf)) {
    throw 'Build kshell.exe before installing the Terminal profile.'
}
if (-not $OutputPath) {
    $localAppData = [Environment]::GetFolderPath('LocalApplicationData')
    if (-not $localAppData) { throw 'The local application-data folder is unavailable.' }
    $OutputPath = Join-Path $localAppData 'Microsoft\Windows Terminal\Fragments\K Shell\kshell.json'
}
if ($FontFace -eq 'auto') {
    $FontFace = 'Consolas'
    foreach ($fontKey in @('HKCU:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts', 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Fonts')) {
        if (Test-Path -LiteralPath $fontKey) {
            $fonts = Get-ItemProperty -LiteralPath $fontKey
            if ($fonts.PSObject.Properties.Name -match '^MesloLGM Nerd Font Mono( Regular)? \(TrueType\)$') { $FontFace = 'MesloLGM Nerd Font Mono'; break }
        }
    }
}
if ($Icons -eq 'auto') { $Icons = if ($FontFace -match 'Nerd Font') { 'nerd' } else { 'unicode' } }

$fragment = [ordered]@{
    profiles = @([ordered]@{
        guid = '{236e02fb-e244-41fe-8206-51277c9be055}'
        name = 'K Shell'
        commandline = '"' + $resolvedExecutable + '" --theme midnight --icons ' + $Icons + ' --suggestions soft'
        startingDirectory = '%USERPROFILE%'
        tabTitle = 'K Shell'
        colorScheme = 'K Shell Midnight'
        font = [ordered]@{ face = $FontFace; size = 13; weight = 'normal' }
        padding = '18, 14, 18, 12'
        cursorShape = 'bar'
        opacity = 100
        useAcrylic = $false
        intenseTextStyle = 'bold'
        antialiasingMode = 'grayscale'
        hidden = $false
    })
    schemes = @([ordered]@{
        name = 'K Shell Midnight'
        background = '#172234'; foreground = '#D6DFEA'
        cursorColor = '#D6DFEA'; selectionBackground = '#30475F'
        black = '#172234'; red = '#E08A90'; green = '#8BCBB2'; yellow = '#D9BA83'
        blue = '#79B9CC'; purple = '#B6AEDB'; cyan = '#8ACBCC'; white = '#D6DFEA'
        brightBlack = '#8B99AE'; brightRed = '#EDABB0'; brightGreen = '#A7DCC6'
        brightYellow = '#E9D0A4'; brightBlue = '#9CCFDE'; brightPurple = '#D0C8EB'
        brightCyan = '#AEDDDD'; brightWhite = '#F0F4F9'
    })
}
$json = $fragment | ConvertTo-Json -Depth 8
if ($Preview) { Write-Output $json; return }
$resolvedOutput = [IO.Path]::GetFullPath($OutputPath)
$parent = [IO.Path]::GetDirectoryName($resolvedOutput)
[IO.Directory]::CreateDirectory($parent) | Out-Null
if (Test-Path -LiteralPath $resolvedOutput) {
    $backupPath = $resolvedOutput + '.' + [DateTime]::UtcNow.ToString('yyyyMMddHHmmssfff') + '.bak'
    Copy-Item -LiteralPath $resolvedOutput -Destination $backupPath -ErrorAction Stop
}
[IO.File]::WriteAllText($resolvedOutput, $json, [Text.UTF8Encoding]::new($false))
Write-Output "Installed K Shell profile: $resolvedOutput"
Write-Output 'Open a new Windows Terminal tab and select K Shell, or run: wt -p "K Shell"'
