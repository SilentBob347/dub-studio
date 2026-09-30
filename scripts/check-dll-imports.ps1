# Checks that every DLL the engines import is shipped, provided by Windows, or installed by First Run:
# downloads the pinned archives with the app's own downloader into an empty root, then walks the import
# tables (crates/dub-server/src/dll_imports.rs). Run it after changing any engine/CUDA pin in setup.rs.
param(
    [Parameter(Mandatory = $true)][string]$StagingRoot
)
$ErrorActionPreference = 'Stop'
New-Item -ItemType Directory -Force -Path $StagingRoot | Out-Null
$env:DUB_DLL_STAGING = (Resolve-Path $StagingRoot).Path
Push-Location (Join-Path $PSScriptRoot '..')
try {
    cargo test -p dub-server --lib dll_imports::tests::downloaded_runtime_can_be_loaded -- --ignored --nocapture
    if ($LASTEXITCODE -ne 0) { throw "DLL import check failed (exit $LASTEXITCODE)" }
} finally {
    Pop-Location
}
