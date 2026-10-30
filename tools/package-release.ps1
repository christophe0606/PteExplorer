param(
    [string]$BuildDirectory = 'build',
    [string]$OutputDirectory = 'dist'
)

$ErrorActionPreference = 'Stop'
$repository = Split-Path -Parent $PSScriptRoot
$runtime = [System.Runtime.InteropServices.RuntimeInformation]
if ($runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
    $platform = 'windows'
    $executable = 'pte-explorer.exe'
    $extension = 'zip'
} elseif ($runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
    $platform = 'macos'
    $executable = 'pte-explorer'
    $extension = 'tar.gz'
} elseif ($runtime::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
    $platform = 'linux'
    $executable = 'pte-explorer'
    $extension = 'tar.gz'
} else {
    throw 'Release packaging supports Windows, macOS and Linux.'
}
$architecture = $runtime::ProcessArchitecture.ToString().ToLowerInvariant()
$packageName = "pte-explorer-$platform-$architecture"
$buildPath = (Resolve-Path -LiteralPath $BuildDirectory).Path
$outputPath = (New-Item -ItemType Directory -Path $OutputDirectory -Force).FullName
$stagingParent = Join-Path $buildPath "package-staging/$([guid]::NewGuid())"
$stagingPath = Join-Path $stagingParent $packageName
New-Item -ItemType Directory -Path $stagingPath -Force | Out-Null

cmake --install $buildPath --config Release --prefix $stagingPath
if ($LASTEXITCODE -ne 0) { throw 'CMake install failed.' }
if ($platform -eq 'macos') {
    $app = Join-Path $stagingPath 'PTE Explorer.app'
    if (-not (Test-Path -LiteralPath (Join-Path $app 'Contents/MacOS/pte-explorer') -PathType Leaf)) {
        throw 'Installed macOS app bundle is missing.'
    }
    python3 (Join-Path $repository 'tools/macos-bundle.py') --app $app --notarize-if-configured
    if ($LASTEXITCODE -ne 0) { throw 'macOS signing/notarization failed.' }
} elseif (-not (Test-Path -LiteralPath (Join-Path $stagingPath "bin/$executable") -PathType Leaf)) {
    throw "Installed executable is missing: bin/$executable"
}
foreach ($file in 'README.md', 'LICENSE', 'THIRD_PARTY.md', 'licenses') {
    Copy-Item -LiteralPath (Join-Path $repository $file) -Destination $stagingPath -Recurse
}

$archive = Join-Path $outputPath "$packageName.$extension"
Push-Location -LiteralPath $stagingParent
try {
    if ($platform -eq 'windows') {
        cmake -E tar cf $archive --format=zip $packageName
    } else {
        cmake -E tar czf $archive --format=gnutar $packageName
    }
    if ($LASTEXITCODE -ne 0) { throw 'Creating the release archive failed.' }
} finally {
    Pop-Location
}
Write-Output "Packaged application: $archive"
