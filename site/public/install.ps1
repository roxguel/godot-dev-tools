# Installs gdt (Godot Development Tools) on Windows.
#
#   irm https://gdt.roxguel.dev/install.ps1 | iex
#
# Options through environment variables (set them before the line above):
#   $env:GDT_VERSION      install this version, for example 0.1.0 (default: the latest release)
#   $env:GDT_INSTALL_DIR  install into this folder (default: %LOCALAPPDATA%\Programs\gdt)
#   $env:GDT_VERIFY_ATTESTATION = '1'   also check the build provenance with `gh attestation verify`
#
# Or, to pass them as parameters:
#   & ([scriptblock]::Create((irm https://gdt.roxguel.dev/install.ps1))) -Version 0.1.0 -InstallDir C:\tools\gdt
#
# It downloads the release archive for this machine from GitHub, checks it against the
# SHA256SUMS file of the same release, and puts gdt.exe in one folder. It does not change
# PATH or anything else. $env:GDT_BASE_URL replaces https://github.com/roxguel/godot-dev-tools
# (for mirrors and tests); with a mirror, trust the build provenance check, not the transport.

function Install-Gdt {
    [CmdletBinding()]
    param(
        [string]$Version = $env:GDT_VERSION,
        [string]$InstallDir = $env:GDT_INSTALL_DIR,
        [switch]$VerifyAttestation = ($env:GDT_VERIFY_ATTESTATION -eq '1')
    )

    $ErrorActionPreference = 'Stop'
    $ProgressPreference = 'SilentlyContinue' # the progress bar makes Windows PowerShell 5.1 downloads very slow
    try { [Net.ServicePointManager]::SecurityProtocol = [Net.ServicePointManager]::SecurityProtocol -bor [Net.SecurityProtocolType]::Tls12 } catch { }

    $custom = [bool]$env:GDT_BASE_URL
    $repoUrl = if ($custom) { $env:GDT_BASE_URL.TrimEnd('/') } else { 'https://github.com/roxguel/godot-dev-tools' }
    $slug = 'roxguel/godot-dev-tools'

    function Fail([string]$message) { throw "gdt installer: $message" }

    # GDT_BASE_URL (mirrors and tests) lifts the transport rules of the default address.
    if ($custom -and $repoUrl -notlike 'https://*') {
        Write-Warning "gdt installer: $repoUrl is not an https:// address, so the download is not protected in transit. Only -VerifyAttestation says where the file was built."
    }

    function Get-LatestTag {
        # Windows PowerShell 5.1 does not load this assembly by itself.
        Add-Type -AssemblyName System.Net.Http
        $url = "$repoUrl/releases/latest"
        $handler = [System.Net.Http.HttpClientHandler]::new()
        $handler.AllowAutoRedirect = $false
        $client = [System.Net.Http.HttpClient]::new($handler)
        try {
            $response = $client.GetAsync($url).GetAwaiter().GetResult()
            $location = $response.Headers.Location
            if (-not $location) { Fail "could not find the latest release ($url); is there a published release yet?" }
            return ($location.ToString() -split '/')[-1]
        } finally { $client.Dispose() }
    }

    if (-not $Version) { $tag = Get-LatestTag } else { $tag = 'v' + $Version.TrimStart('v') }
    # The tag ends up in URLs and file names, so it is held to a strict shape.
    if ($tag -notmatch '^v[0-9]+\.[0-9]+\.[0-9]+([-+.][0-9A-Za-z.+-]+)?$') { Fail "not a version: $tag" }

    $arch = [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture.ToString()
    $target = switch ($arch) {
        'X64'   { 'x86_64-pc-windows-msvc' }
        'Arm64' { 'aarch64-pc-windows-msvc' }
        default { Fail "no build for Windows on $arch" }
    }
    $name = "gdt-$tag-$target"
    $archive = "$name.zip"

    if (-not $InstallDir) {
        if (-not $env:LOCALAPPDATA) { Fail 'LOCALAPPDATA is not set; say where to install with -InstallDir' }
        $InstallDir = Join-Path $env:LOCALAPPDATA 'Programs\gdt'
    }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ("gdt-install-" + [Guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    try {
        Write-Host "Installing gdt $tag for $target"
        $base = "$repoUrl/releases/download/$tag"
        try {
            Invoke-WebRequest -UseBasicParsing -Uri "$base/$archive" -OutFile (Join-Path $tmp $archive)
            Invoke-WebRequest -UseBasicParsing -Uri "$base/SHA256SUMS" -OutFile (Join-Path $tmp 'SHA256SUMS')
        } catch { Fail "could not download from $base ($($_.Exception.Message))" }

        $expected = $null
        foreach ($line in Get-Content (Join-Path $tmp 'SHA256SUMS')) {
            if ($line -match '^([0-9a-fA-F]{64})\s+\*?(.+)$' -and $Matches[2].Trim() -eq $archive) { $expected = $Matches[1]; break }
        }
        if (-not $expected) { Fail "$archive is not listed in SHA256SUMS; nothing was installed" }
        $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath (Join-Path $tmp $archive)).Hash
        if ($actual -ne $expected.ToUpperInvariant()) {
            Fail "checksum mismatch for $archive (expected $expected, got $actual); nothing was installed"
        }
        Write-Host 'Checksum ok'

        if ($VerifyAttestation) {
            if (-not (Get-Command gh -ErrorAction SilentlyContinue)) { Fail '-VerifyAttestation needs the GitHub CLI (gh)' }
            # Not any attestation of the repository: the one that this repository's release
            # workflow made, on a GitHub-hosted runner, for this very tag. Windows PowerShell 5.1
            # turns what a program writes to stderr into an error while 'Stop' is set, so the
            # call runs under 'Continue' and the exit code decides.
            $previous = $ErrorActionPreference
            $ErrorActionPreference = 'Continue'
            try {
                $null = & gh attestation verify (Join-Path $tmp $archive) --repo $slug `
                    --signer-workflow "$slug/.github/workflows/release.yml" `
                    --source-ref "refs/tags/$tag" `
                    --deny-self-hosted-runners 2>&1
                $code = $LASTEXITCODE
            } finally { $ErrorActionPreference = $previous }
            if ($code -ne 0) { Fail "the build provenance of $archive could not be verified (it needs a recent GitHub CLI, signed in); nothing was installed" }
            Write-Host 'Build provenance ok'
        } else {
            Write-Host '(The checksum shows the download is intact. -VerifyAttestation also shows where it was built.)'
        }

        # Only the one expected file is taken out of the archive, to a path chosen here, so
        # nothing in the archive decides where anything is written. Entry names use '/' or,
        # in archives made by Windows PowerShell 5.1, '\'.
        Add-Type -AssemblyName System.IO.Compression.FileSystem
        $exe = Join-Path $tmp 'gdt.exe'
        $zip = [IO.Compression.ZipFile]::OpenRead((Join-Path $tmp $archive))
        try {
            $entry = $zip.Entries | Where-Object { $_.FullName.Replace('\', '/') -eq "$name/gdt.exe" } | Select-Object -First 1
            if (-not $entry) { Fail "$archive has no gdt.exe" }
            [IO.Compression.ZipFileExtensions]::ExtractToFile($entry, $exe, $true)
        } finally { $zip.Dispose() }

        New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
        $final = Join-Path $InstallDir 'gdt.exe'
        # Copy beside the final name, then rename, so a failed copy never leaves half a gdt.exe.
        $staged = Join-Path $InstallDir (".gdt.new." + [Guid]::NewGuid().ToString('N'))
        try {
            Copy-Item -LiteralPath $exe -Destination $staged -Force
            Move-Item -LiteralPath $staged -Destination $final -Force
        } catch {
            Remove-Item -LiteralPath $staged -Force -ErrorAction SilentlyContinue
            Fail "could not write $final; if gdt is running, close it and try again ($($_.Exception.Message))"
        }

        # Asking for the version is only a courtesy; whatever it does must not undo a finished install
        # (Windows PowerShell 5.1 raises an error for a program's stderr while 'Stop' is set).
        $installed = try { & $final --version 2>$null } catch { $null }
        if (-not $installed) { $installed = 'gdt' }
        Write-Host "Installed $installed in $InstallDir"

        $onPath = $false
        foreach ($entry in ($env:Path -split ';')) {
            if ($entry -and ($entry.TrimEnd('\') -ieq $InstallDir.TrimEnd('\'))) { $onPath = $true }
        }
        if (-not $onPath) {
            Write-Host ''
            Write-Host "$InstallDir is not on your Path. To add it, open `"Edit environment variables for your account`""
            Write-Host '(Start menu) and add it to the user variable Path, then open a new terminal.'
            Write-Host 'The installer changes nothing else.'
        }
    } finally {
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }
}

Install-Gdt @args
