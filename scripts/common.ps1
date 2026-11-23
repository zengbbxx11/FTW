function Invoke-Checked {
    param(
        [Parameter(Mandatory = $true)][string]$Program,
        [Parameter(Mandatory = $true)][string[]]$Arguments
    )

    & $Program @Arguments
    if ($LASTEXITCODE -ne 0) {
        throw "$Program $($Arguments -join ' ') exited with code $LASTEXITCODE."
    }
}

$rustInfo = rustc -vV
if ($LASTEXITCODE -ne 0 -or $rustInfo -notcontains 'host: x86_64-pc-windows-msvc') {
    throw 'FTW requires an x86_64-pc-windows-msvc Rust toolchain.'
}
if ($env:CARGO_BUILD_TARGET) {
    throw 'Clear CARGO_BUILD_TARGET so the dev and test scripts use the native x64 MSVC target.'
}
