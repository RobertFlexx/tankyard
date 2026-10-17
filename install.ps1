# Install a released Windows build for the current user.
[CmdletBinding()]
param(
    [string]$Version = "latest",
    [string]$InstallDir = "$env:LOCALAPPDATA\Programs\Tank Yard",
    [switch]$NoShortcut,
    [switch]$NoPath,
    [switch]$Server
)
$ErrorActionPreference = "Stop"
$ProgressPreference = "SilentlyContinue"
$Repository = "RobertFlexx/tankyard"
$WorkDir = Join-Path ([IO.Path]::GetTempPath()) ("tankyard-" + [guid]::NewGuid())
try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "Use install.sh on Linux or macOS."
    }
    if (-not [Environment]::Is64BitOperatingSystem) { throw "Tank Yard needs 64-bit Windows." }
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Write-Host "Tank Yard" -ForegroundColor Cyan
    if ($Version -eq "latest") {
        $Release = Invoke-RestMethod "https://api.github.com/repos/$Repository/releases/latest" -Headers @{ "User-Agent" = "TankYard-Installer" }
        $Version = $Release.tag_name
    }
    if ($Version -notmatch '^v\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$') { throw "Use a release tag such as v0.2.7." }
    $RootName = "tankyard-$Version-windows-x86_64"
    $Archive = "$RootName.zip"
    $Base = "https://github.com/$Repository/releases/download/$Version"
    New-Item -ItemType Directory -Force $WorkDir | Out-Null
    Write-Host "Downloading $Version for Windows..."
    Invoke-WebRequest "$Base/SHA256SUMS" -OutFile "$WorkDir\SHA256SUMS" -UseBasicParsing
    Invoke-WebRequest "$Base/$Archive" -OutFile "$WorkDir\$Archive" -UseBasicParsing
    $Line = Get-Content "$WorkDir\SHA256SUMS" | Where-Object { $_ -match ("^[0-9a-f]{64}  " + [regex]::Escape($Archive) + "$") }
    if (@($Line).Count -ne 1) { throw "No checksum for $Archive." }
    $Expected = ($Line -split '\s+')[0]
    $Actual = (Get-FileHash "$WorkDir\$Archive" -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) { throw "Checksum mismatch. The existing installation was kept." }
    Write-Host "Download verified."
    Expand-Archive "$WorkDir\$Archive" "$WorkDir\unpacked"
    $Payload = Join-Path "$WorkDir\unpacked" $RootName
    $Binaries = if ($Server) { @("tankyard-server", "tankyard-relay") } else { @("tankyard") }
    foreach ($Binary in $Binaries) {
        if (-not (Test-Path "$Payload\$Binary.exe" -PathType Leaf)) { throw "The archive is missing $Binary.exe." }
    }
    & "$Payload\$($Binaries[0]).exe" --version
    if ($LASTEXITCODE -ne 0) { throw "This build cannot run on this computer." }
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    foreach ($Binary in $Binaries) {
        $Destination = Join-Path $InstallDir "$Binary.exe"
        Copy-Item "$Payload\$Binary.exe" "$Destination.new" -Force
        try { Move-Item "$Destination.new" $Destination -Force }
        catch { Remove-Item "$Destination.new" -Force; throw "Close Tank Yard and run the installer again. $($_.Exception.Message)" }
    }
    if (-not $NoPath) {
        $UserPath = [Environment]::GetEnvironmentVariable("Path", "User")
        if ($null -eq $UserPath) { $UserPath = "" }
        if (($UserPath -split ';') -notcontains $InstallDir) {
            [Environment]::SetEnvironmentVariable("Path", ($UserPath.TrimEnd(';') + ";" + $InstallDir).TrimStart(';'), "User")
        }
        $env:Path = "$InstallDir;$env:Path"
    }
    if (-not $NoShortcut -and -not $Server) {
        $Shell = New-Object -ComObject WScript.Shell
        $Shortcut = $Shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath("Programs")) "Tank Yard.lnk"))
        $Shortcut.TargetPath = Join-Path $InstallDir "tankyard.exe"
        $Shortcut.WorkingDirectory = $InstallDir
        $Shortcut.Save()
    }
    Write-Host "Installed to $InstallDir" -ForegroundColor Green
    if ($Server) {
        Write-Host "Ready. Run tankyard-server --help in a new terminal."
    } elseif ($NoShortcut) {
        Write-Host "Ready. Run tankyard in a new terminal."
    } else {
        Write-Host "Ready. Open Tank Yard from Start, or run tankyard in a new terminal."
    }
}
finally {
    if (Test-Path $WorkDir) { Remove-Item -Recurse -Force $WorkDir }
}
