#requires -Version 5.1
<#
.SYNOPSIS
    打包 xRename：改版本号 → 跑测试 → release 编译 → 收集产物 → 打 zip。

.EXAMPLE
    .\pack.ps1 0.2.0

.EXAMPLE
    .\pack.ps1 0.2.0 -SkipTests -NoZip

.NOTES
    版本号写在 Cargo.toml 的 [package].version，exe 属性里的文件版本/产品版本
    和界面标题栏都由它而来，所以这里改一处就够了。
#>
[CmdletBinding()]
param(
    [Parameter(Position = 0, HelpMessage = "版本号，例如 0.2.0")]
    [string]$Version,

    # 跳过测试，赶时间时用
    [switch]$SkipTests,

    # 只出 exe 不压缩
    [switch]$NoZip,

    # 先 cargo clean 再编（改完依赖版本之类的时候用，很慢）
    [switch]$Clean
)

$ErrorActionPreference = 'Stop'
Set-StrictMode -Version Latest

$root = $PSScriptRoot
$manifest = Join-Path $root 'Cargo.toml'

function Write-Step([string]$text) {
    Write-Host ''
    Write-Host "==> $text" -ForegroundColor Cyan
}

function Get-ManifestVersion {
    $lines = Get-Content -LiteralPath $manifest -Encoding UTF8
    $inPackage = $false
    foreach ($line in $lines) {
        if ($line -match '^\s*\[(.+)\]\s*$') {
            $inPackage = ($Matches[1] -eq 'package')
            continue
        }
        if ($inPackage -and $line -match '^\s*version\s*=\s*"([^"]+)"') {
            return $Matches[1]
        }
    }
    throw "Cargo.toml 里没找到 [package].version"
}

function Set-ManifestVersion([string]$newVersion) {
    $lines = Get-Content -LiteralPath $manifest -Encoding UTF8
    $inPackage = $false
    $done = $false
    for ($i = 0; $i -lt $lines.Count; $i++) {
        if ($lines[$i] -match '^\s*\[(.+)\]\s*$') {
            $inPackage = ($Matches[1] -eq 'package')
            continue
        }
        if ($inPackage -and -not $done -and $lines[$i] -match '^\s*version\s*=') {
            $lines[$i] = "version = `"$newVersion`""
            $done = $true
        }
    }
    if (-not $done) { throw "Cargo.toml 里没找到 [package].version" }
    # 写回时保持 UTF-8 无 BOM、LF 换行，别把 Cargo.toml 改出新花样
    $text = ($lines -join "`n") + "`n"
    [System.IO.File]::WriteAllText($manifest, $text, (New-Object System.Text.UTF8Encoding($false)))
}

function Invoke-Cargo {
    param([string[]]$CargoArgs)
    Write-Host "`$ cargo $($CargoArgs -join ' ')" -ForegroundColor DarkGray
    # cargo 的进度是往 stderr 写的，别让 Stop 把它当成异常
    $prevEap = $ErrorActionPreference
    $ErrorActionPreference = 'Continue'
    try {
        & cargo @CargoArgs
    }
    finally {
        $ErrorActionPreference = $prevEap
    }
    if ($LASTEXITCODE -ne 0) {
        throw "cargo $($CargoArgs[0]) 失败（退出码 $LASTEXITCODE）"
    }
}

# ---------------------------------------------------------------- 版本号

if (-not $Version) {
    $current = Get-ManifestVersion
    Write-Host "打包 xRename" -ForegroundColor White
    Write-Host "  当前版本：$current"
    Write-Host ''
    Write-Host "用法：.\pack.ps1 <版本号>  [ -SkipTests ] [ -NoZip ] [ -Clean ]"
    Write-Host "例如：.\pack.ps1 0.2.0"
    exit 1
}

$Version = $Version.Trim()
if ($Version.StartsWith('v') -or $Version.StartsWith('V')) { $Version = $Version.Substring(1) }
if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z][0-9A-Za-z.-]*)?$') {
    throw "版本号格式不对：'$Version'。要形如 0.2.0 或 0.2.0-beta.1"
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw '没找到 cargo，先把 Rust 工具链的 bin 目录加进 PATH'
}

$oldVersion = Get-ManifestVersion
Write-Step "版本号 $oldVersion → $Version"
if ($oldVersion -ne $Version) {
    Set-ManifestVersion $Version
    Write-Host "   已更新 $manifest"
} else {
    Write-Host '   版本号没变，跳过'
}

# ---------------------------------------------------------------- 编译

Push-Location $root
try {
    if ($Clean) {
        Write-Step 'cargo clean'
        Invoke-Cargo @('clean')
    }

    if (-not $SkipTests) {
        Write-Step '跑测试'
        Invoke-Cargo @('test', '-q')
    } else {
        Write-Host ''
        Write-Host '==> 跳过测试（-SkipTests）' -ForegroundColor DarkYellow
    }

    Write-Step 'release 编译'
    Invoke-Cargo @('build', '--release')

    # ------------------------------------------------------------ 收产物

    Write-Step '整理产物'
    $exe = Join-Path $root 'target\release\xrename.exe'
    if (-not (Test-Path -LiteralPath $exe)) {
        throw "没找到编译产物：$exe"
    }

    $dist = Join-Path $root 'dist'
    $folderName = "xRename-v$Version-win-x64"
    $stage = Join-Path $dist $folderName
    if (Test-Path -LiteralPath $stage) {
        Remove-Item -LiteralPath $stage -Recurse -Force
    }
    New-Item -ItemType Directory -Path $stage -Force | Out-Null

    $outExe = Join-Path $stage 'xRename.exe'
    Copy-Item -LiteralPath $exe -Destination $outExe -Force

    # 留个版本说明，用户拿到压缩包不用猜
    $readme = @"
xRename v$Version
批量重命名工具（脚本 / 规则两种改法）

用法：双击 xRename.exe，把文件或文件夹拖进窗口。

支持的脚本语言：JavaScript / C++ / Python / Rust / Java
脚本里写一个函数体，参数 item 和 index，返回新文件名（不含扩展名）。

打包时间：$(Get-Date -Format 'yyyy-MM-dd HH:mm:ss')
"@
    [System.IO.File]::WriteAllText(
        (Join-Path $stage 'README.txt'),
        ($readme + "`r`n"),
        (New-Object System.Text.UTF8Encoding($true))
    )

    $exeSize = [math]::Round((Get-Item -LiteralPath $outExe).Length / 1MB, 1)
    Write-Host ("   {0}（{1} MB）" -f $outExe, $exeSize)

    $zipPath = $null
    if (-not $NoZip) {
        Write-Step '压缩'
        $zipPath = Join-Path $dist "$folderName.zip"
        if (Test-Path -LiteralPath $zipPath) {
            Remove-Item -LiteralPath $zipPath -Force
        }
        Compress-Archive -Path $stage -DestinationPath $zipPath -CompressionLevel Optimal
        $zipSize = [math]::Round((Get-Item -LiteralPath $zipPath).Length / 1MB, 1)
        Write-Host ("   {0}（{1} MB）" -f $zipPath, $zipSize)
    }

    # ------------------------------------------------------------ 汇报

    $hash = (Get-FileHash -LiteralPath $outExe -Algorithm SHA256).Hash
    Write-Step '完成'
    Write-Host "   版本    v$Version"
    Write-Host "   目录    $stage"
    if ($zipPath) { Write-Host "   压缩包  $zipPath" }
    Write-Host "   SHA256  $hash"
    Write-Host ''
    Write-Host "   提示：target\release\xrename.exe 里已经带了图标和版本号，" -ForegroundColor DarkGray
    Write-Host "         dist 下的 xRename.exe 就是改名后可以直接发出去的那份。" -ForegroundColor DarkGray
}
finally {
    Pop-Location
}
