# Installs the kulisa tool on Windows:
#
#   irm https://github.com/nikolchaa/Kulisa/releases/latest/download/install.ps1 | iex
#
# KULISA_HOME            where it goes (default %LOCALAPPDATA%\Programs\Kulisa)
# KULISA_RELEASES        where it downloads from (default the latest release)
# KULISA_NO_MODIFY_PATH  leave the PATH alone

# In a block, so a download of this script cut short runs nothing and no variables stay behind.
& {
    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue'
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

    $releases = $env:KULISA_RELEASES
    if (-not $releases) { $releases = 'https://github.com/nikolchaa/Kulisa/releases/latest/download' }
    $kulisaHome = $env:KULISA_HOME
    if (-not $kulisaHome) { $kulisaHome = Join-Path $env:LOCALAPPDATA 'Programs\Kulisa' }
    $archive = 'kulisa-windows.zip'
    if ((Test-Path $kulisaHome) -and -not (Test-Path (Join-Path $kulisaHome 'kulisa.exe'))) {
        throw "$kulisaHome is in the way and is not a kulisa install; set KULISA_HOME to another folder"
    }

    $work = Join-Path ([IO.Path]::GetTempPath()) ("kulisa-install-" + [Guid]::NewGuid())
    New-Item -ItemType Directory -Path $work | Out-Null
    try {
        Write-Host "downloading $archive..."
        $file = Join-Path $work $archive
        Invoke-WebRequest -UseBasicParsing -Uri "$releases/$archive" -OutFile $file
        $sums = Join-Path $work 'SHA256SUMS'
        Invoke-WebRequest -UseBasicParsing -Uri "$releases/SHA256SUMS" -OutFile $sums

        $want = $null
        foreach ($line in Get-Content $sums) {
            $parts = $line -split '\s+'
            if ($parts.Count -ge 2 -and $parts[1].TrimStart('*') -eq $archive) { $want = $parts[0] }
        }
        $got = (Get-FileHash -Algorithm SHA256 $file).Hash
        if (-not $want -or $got -ne $want) { throw "the checksum of $archive is wrong; try again" }

        Expand-Archive -Path $file -DestinationPath $work
        if (Test-Path $kulisaHome) { Remove-Item -Recurse -Force $kulisaHome }
        New-Item -ItemType Directory -Force -Path (Split-Path $kulisaHome) | Out-Null
        Move-Item (Join-Path $work 'kulisa') $kulisaHome
    } finally {
        Remove-Item -Recurse -Force $work -ErrorAction SilentlyContinue
    }
    Write-Host "installed in $kulisaHome"

    $path = [Environment]::GetEnvironmentVariable('Path', 'User')
    if (($path -split ';') -contains $kulisaHome) { return }
    if ($env:KULISA_NO_MODIFY_PATH) {
        Write-Host "to use it from anywhere, add that folder to your PATH"
        return
    }
    [Environment]::SetEnvironmentVariable('Path', ($(if ($path) { "$path;" }) + $kulisaHome), 'User')
    Write-Host "added it to your PATH; open a new terminal and run: kulisa help"
}
