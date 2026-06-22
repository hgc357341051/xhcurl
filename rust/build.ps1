# +----------------------------------------------------------------------+
# | XHCurl 扩展 - 构建脚本                                                |
# | 自动化构建、测试、打包流程                                             |
# +----------------------------------------------------------------------+

#Requires -Version 5.1
<#
.SYNOPSIS
    XHCurl Rust 扩展构建脚本

.DESCRIPTION
    提供完整的构建、测试、打包、清理功能

.EXAMPLE
    .\build.ps1 build        # 构建扩展
    .\build.ps1 test         # 运行测试
    .\build.ps1 release      # 发布构建
    .\build.ps1 clean        # 清理构建产物
    .\build.ps1 install      # 安装到 PHP 扩展目录
#>

param(
    [Parameter(Position=0)]
    [ValidateSet("build", "test", "release", "clean", "install", "check", "all")]
    [string]$Action = "build",

    # PHP 版本（用于选择 ext-php-rs 兼容版本）
    [string]$PhpVersion = "8.2",

    # 目标平台
    [string]$Target = "",

    # 是否启用 PHP 扩展功能
    [switch]$WithPhp
)

# 错误时停止
$ErrorActionPreference = "Stop"

# 脚本所在目录
$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectDir = $ScriptDir

# 进入项目目录
Set-Location $ProjectDir

# 输出信息函数
function Write-Info {
    param([string]$Message)
    Write-Host "[INFO] $Message" -ForegroundColor Cyan
}

function Write-Success {
    param([string]$Message)
    Write-Host "[OK] $Message" -ForegroundColor Green
}

function Write-Error-Msg {
    param([string]$Message)
    Write-Host "[ERROR] $Message" -ForegroundColor Red
}

# 检查 Rust 工具链
function Check-RustToolchain {
    Write-Info "检查 Rust 工具链..."

    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Error-Msg "未找到 cargo，请先安装 Rust 工具链"
        Write-Info "安装方法: https://rustup.rs/"
        exit 1
    }

    $rustVersion = cargo --version
    Write-Success "Rust 工具链: $rustVersion"
}

# 检查 PHP 开发环境
function Check-PhpDev {
    if (-not $WithPhp) {
        Write-Info "跳过 PHP 开发环境检查（未启用 --with-php）"
        return
    }

    Write-Info "检查 PHP 开发环境..."

    $phpConfig = Get-Command php-config -ErrorAction SilentlyContinue
    if (-not $phpConfig) {
        Write-Error-Msg "未找到 php-config，请安装 PHP 开发包"
        Write-Info "Ubuntu: sudo apt install php-dev"
        Write-Info "CentOS: sudo yum install php-devel"
        exit 1
    }

    $phpVersion = & php-config --version
    Write-Success "PHP 版本: $phpVersion"
}

# 构建扩展
function Build-Extension {
    Write-Info "开始构建 XHCurl 扩展..."

    Check-RustToolchain

    # 构建参数
    $buildArgs = @("build")

    # 如果启用 PHP 扩展
    if ($WithPhp) {
        $buildArgs += @("--features", "php")
    }

    # 指定目标平台
    if ($Target) {
        $buildArgs += @("--target", $Target)
    }

    # 执行构建
    & cargo @buildArgs

    if ($LASTEXITCODE -eq 0) {
        Write-Success "构建成功！"
        Write-Info "产物位置: target/debug/"
    } else {
        Write-Error-Msg "构建失败"
        exit 1
    }
}

# 发布构建
function Build-Release {
    Write-Info "开始发布构建（optimized）..."

    Check-RustToolchain

    $buildArgs = @("build", "--release")

    if ($WithPhp) {
        $buildArgs += @("--features", "php")
    }

    if ($Target) {
        $buildArgs += @("--target", $Target)
    }

    & cargo @buildArgs

    if ($LASTEXITCODE -eq 0) {
        Write-Success "发布构建成功！"
        Write-Info "产物位置: target/release/"

        # 显示产物信息
        $libName = if ($IsWindows -or $env:OS -eq "Windows_NT") {
            "xhcurl.dll"
        } else {
            "libxhcurl.so"
        }

        $libPath = "target/release/$libName"
        if (Test-Path $libPath) {
            $size = (Get-Item $libPath).Length / 1KB
            Write-Info "产物大小: $([math]::Round($size, 2)) KB"
        }
    } else {
        Write-Error-Msg "发布构建失败"
        exit 1
    }
}

# 运行测试
function Run-Tests {
    Write-Info "运行测试..."

    Check-RustToolchain

    & cargo test --verbose

    if ($LASTEXITCODE -eq 0) {
        Write-Success "所有测试通过！"
    } else {
        Write-Error-Msg "测试失败"
        exit 1
    }
}

# 代码检查
function Run-Check {
    Write-Info "运行代码检查..."

    Check-RustToolchain

    & cargo check --all-targets

    if ($LASTEXITCODE -eq 0) {
        Write-Success "代码检查通过！"
    } else {
        Write-Error-Msg "代码检查失败"
        exit 1
    }

    # 运行 clippy（如果安装了）
    $clippy = Get-Command cargo-clippy -ErrorAction SilentlyContinue
    if ($clippy) {
        Write-Info "运行 Clippy 静态分析..."
        & cargo clippy -- -D warnings

        if ($LASTEXITCODE -eq 0) {
            Write-Success "Clippy 检查通过！"
        }
    }
}

# 清理构建产物
function Clean-Build {
    Write-Info "清理构建产物..."

    & cargo clean

    if ($LASTEXITCODE -eq 0) {
        Write-Success "清理完成！"
    }
}

# 安装到 PHP 扩展目录
function Install-Extension {
    Write-Info "安装扩展到 PHP..."

    if (-not $WithPhp) {
        Write-Error-Msg "安装需要 --with-php 参数"
        exit 1
    }

    # 获取 PHP 扩展目录
    $phpExtDir = & php-config --extension-dir
    if (-not $phpExtDir) {
        Write-Error-Msg "无法获取 PHP 扩展目录"
        exit 1
    }

    # 确定构建产物路径
    $libName = if ($IsWindows -or $env:OS -eq "Windows_NT") {
        "xhcurl.dll"
    } else {
        "libxhcurl.so"
    }

    $buildPath = "target/release/$libName"
    if (-not (Test-Path $buildPath)) {
        Write-Error-Msg "构建产物不存在，请先运行: .\build.ps1 release --with-php"
        exit 1
    }

    # 复制到 PHP 扩展目录
    $destPath = Join-Path $phpExtDir $libName
    Copy-Item $buildPath $destPath -Force

    Write-Success "已安装到: $destPath"
    Write-Info "请在 php.ini 中添加: extension=$libName"
}

# 全部执行
function Run-All {
    Clean-Build
    Run-Check
    Run-Tests
    Build-Release
}

# 主逻辑
Write-Host "========================================" -ForegroundColor Yellow
Write-Host "  XHCurl 扩展构建脚本" -ForegroundColor Yellow
Write-Host "========================================" -ForegroundColor Yellow
Write-Host ""

switch ($Action) {
    "build"   { Build-Extension }
    "test"    { Run-Tests }
    "release" { Build-Release }
    "clean"   { Clean-Build }
    "install" { Install-Extension }
    "check"   { Run-Check }
    "all"     { Run-All }
    default   { Build-Extension }
}

Write-Host ""
Write-Success "操作完成: $Action"
