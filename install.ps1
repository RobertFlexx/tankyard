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
$Attempts = 6
if ($env:TANKYARD_DOWNLOAD_ATTEMPTS -match '^\d+$') { $Attempts = [Math]::Max(1, [int]$env:TANKYARD_DOWNLOAD_ATTEMPTS) }

# GitHub's release servers sometimes answer 502/503/504 or drop connections
# under load. Retry those with growing pauses; fail at once on 404 and other
# permanent answers.
function Test-Transient($ErrorRecord) {
    $Code = 0
    $Response = $ErrorRecord.Exception.Response
    if ($null -ne $Response) { $Code = [int]$Response.StatusCode }
    return $Code -eq 0 -or $Code -eq 408 -or $Code -eq 429 -or $Code -ge 500
}

function Save-Download([string]$Uri, [string]$OutFile, [int]$Tries = $Attempts, [hashtable]$Headers = @{}) {
    $Pause = 2
    for ($Try = 1; ; $Try++) {
        try {
            Invoke-WebRequest -Uri $Uri -OutFile $OutFile -Headers $Headers -UseBasicParsing
            return
        }
        catch {
            $Code = 0
            $Response = $_.Exception.Response
            if ($null -ne $Response) { $Code = [int]$Response.StatusCode }
            if (-not (Test-Transient $_) -or $Try -ge $Tries) { throw }
            $Reason = if ($Code -eq 0) { "no response" } else { "HTTP $Code" }
            Write-Host "The download server is busy ($Reason); retrying in $Pause s..." -ForegroundColor Yellow
            Start-Sleep -Seconds $Pause
            $Pause = [Math]::Min($Pause * 2, 30)
        }
    }
}

# A file from release $Version, or from the latest release. github.com's
# download links sometimes answer 504 for long stretches while GitHub's API
# serves the same files, so after two busy answers the file, and every later
# one, comes through the API instead.
$ViaApi = $false
$ReleaseInfo = $null
function Save-ReleaseFile([string]$Name, [string]$OutFile) {
    if (-not $script:ViaApi) {
        $Uri = if ($Version -eq "latest") { "$Releases/latest/download/$Name" } else { "$Releases/download/$Version/$Name" }
        try { Save-Download $Uri $OutFile 2; return }
        catch { if (-not (Test-Transient $_)) { throw } }
        Write-Host "Downloading through the GitHub API instead." -ForegroundColor Yellow
        $script:ViaApi = $true
    }
    if ($null -eq $script:ReleaseInfo) {
        $Release = if ($Version -eq "latest") { "latest" } else { "tags/$Version" }
        Save-Download "https://api.github.com/repos/$Repository/releases/$Release" "$WorkDir\release.json" $Attempts @{ Accept = "application/vnd.github+json" }
        $script:ReleaseInfo = Get-Content -Raw "$WorkDir\release.json" | ConvertFrom-Json
    }
    $Asset = @($script:ReleaseInfo.assets | Where-Object { $_.name -eq $Name })[0]
    if ($null -eq $Asset) { throw "The release has no $Name." }
    Save-Download $Asset.url $OutFile $Attempts @{ Accept = "application/octet-stream" }
}

try {
    if ([Environment]::OSVersion.Platform -ne [PlatformID]::Win32NT) {
        throw "Use install.sh on Linux or macOS."
    }
    if (-not [Environment]::Is64BitOperatingSystem) { throw "Tank Yard needs 64-bit Windows." }
    # TLS 1.3 where .NET offers it, always TLS 1.2.
    $Protocols = [Net.SecurityProtocolType]::Tls12
    if ([Enum]::GetNames([Net.SecurityProtocolType]) -contains "Tls13") { $Protocols = $Protocols -bor [Net.SecurityProtocolType]::Tls13 }
    [Net.ServicePointManager]::SecurityProtocol = $Protocols
    Write-Host "Tank Yard" -ForegroundColor Cyan
    if ($Version -ne "latest" -and $Version -notmatch '^v\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?$') { throw "Use a release tag such as v0.2.9." }
    New-Item -ItemType Directory -Force $WorkDir | Out-Null
    $Releases = "https://github.com/$Repository/releases"
    if ($Version -eq "latest") {
        # The latest checksum list names its archives, and with them the
        # version: no GitHub API request, which is rate limited.
        try { Save-ReleaseFile SHA256SUMS "$WorkDir\SHA256SUMS" }
        catch { throw "Could not reach the latest release. Check your connection, or try again in a minute. $($_.Exception.Message)" }
        $Named = Get-Content "$WorkDir\SHA256SUMS" | ForEach-Object {
            if ($_ -match '^[0-9a-f]{64}  tankyard-(v\d+\.\d+\.\d+(-[A-Za-z0-9.-]+)?)-windows-x86_64\.zip$') { $Matches[1] }
        } | Select-Object -First 1
        if (-not $Named) { throw "The latest release has no Windows build." }
        $Version = $Named
    } else {
        try { Save-ReleaseFile SHA256SUMS "$WorkDir\SHA256SUMS" }
        catch { throw "Release $Version or its checksums are unavailable. $($_.Exception.Message)" }
    }
    $RootName = "tankyard-$Version-windows-x86_64"
    $Archive = "$RootName.zip"
    Write-Host "Downloading $Version for Windows..."
    try { Save-ReleaseFile $Archive "$WorkDir\$Archive" }
    catch { throw "Download failed; the existing installation was kept. Run the installer again to retry. $($_.Exception.Message)" }
    $Line = Get-Content "$WorkDir\SHA256SUMS" | Where-Object { $_ -match ("^[0-9a-f]{64}  " + [regex]::Escape($Archive) + "$") }
    if (@($Line).Count -ne 1) { throw "No checksum for $Archive." }
    $Expected = ($Line -split '\s+')[0]
    $Actual = (Get-FileHash "$WorkDir\$Archive" -Algorithm SHA256).Hash.ToLowerInvariant()
    if ($Actual -ne $Expected) { throw "Checksum mismatch. The existing installation was kept." }
    Write-Host "Download verified."
    # Far faster than Expand-Archive; entries outside the folder are refused.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    [IO.Compression.ZipFile]::ExtractToDirectory("$WorkDir\$Archive", "$WorkDir\unpacked")
    $Payload = Join-Path "$WorkDir\unpacked" $RootName
    [string[]]$Binaries = if ($Server) { @("tankyard-server", "tankyard-relay") } else { @("tankyard") }
    foreach ($Binary in $Binaries) {
        if (-not (Test-Path "$Payload\$Binary.exe" -PathType Leaf)) { throw "The archive is missing $Binary.exe." }
    }
    & "$Payload\$($Binaries[0]).exe" --version
    if ($LASTEXITCODE -ne 0) { throw "This build cannot run on this computer." }
    New-Item -ItemType Directory -Force $InstallDir | Out-Null
    if (-not $Server) {
        Copy-Item "$Payload\tankyard.ico" (Join-Path $InstallDir "tankyard.ico") -Force
    }
    foreach ($Binary in $Binaries) {
        $Destination = Join-Path $InstallDir "$Binary.exe"
        Copy-Item "$Payload\$Binary.exe" "$Destination.new" -Force
        # A running game holds its file briefly after closing; wait a moment.
        for ($Try = 1; ; $Try++) {
            try { Move-Item "$Destination.new" $Destination -Force; break }
            catch {
                if ($Try -ge 5) {
                    Remove-Item "$Destination.new" -Force
                    throw "Close Tank Yard and run the installer again. $($_.Exception.Message)"
                }
                Start-Sleep -Milliseconds (400 * $Try)
            }
        }
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
        $Shortcut.IconLocation = (Join-Path $InstallDir "tankyard.ico") + ",0"
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
