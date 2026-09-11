# Build HyloPDF from this checkout and install it as a real app. One command,
# nothing to piece together: pdfium, the release build, the installer, and the
# install itself. macOS and Linux have scripts/install.sh.
$ErrorActionPreference = "Stop"
Set-Location (Join-Path $PSScriptRoot "..")

& (Join-Path $PSScriptRoot "pdfium.ps1")
if (-not (Get-Command cargo-packager -ErrorAction SilentlyContinue)) {
    cargo install cargo-packager --locked
}
cargo build --release
cargo packager --release --formats nsis

# SmartScreen will warn — the installer is not signed. "More info" → "Run anyway".
$setup = Get-ChildItem target\release\*-setup.exe | Select-Object -First 1
Start-Process -FilePath $setup.FullName -Wait
Write-Host "Installed HyloPDF — it is in the Start menu."
