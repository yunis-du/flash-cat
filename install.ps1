# Auto-elevate to admin rights if not already running as admin
if (-NOT ([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole] "Administrator")) {
    Write-Host "Requesting administrator privileges..."
    $arguments = "-NoProfile -ExecutionPolicy Bypass -File `"$PSCommandPath`" -ExecutionFromElevated"
    Start-Process powershell.exe -ArgumentList $arguments -Verb RunAs
    Exit
}

# Set TLS to 1.2
[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12

# Colors for output
$Red = "`e[31m"
$Green = "`e[32m"
$Blue = "`e[36m"
$Yellow = "`e[33m"
$Reset = "`e[0m"

# Messages
$EN_MESSAGES = @(
    "Starting installation...",
    "Detected architecture:",
    "Only 64-bit Windows is supported",
    "Latest version:",
    "Creating installation directory...",
    "Downloading latest release from:",
    "Failed to download binary:",
    "Downloaded file not found",
    "Installing binary...",
    "Failed to install binary:",
    "Adding to PATH...",
    "Cleaning up...",
    "Installation completed successfully!",
    "You can now use 'flash-cat' directly"
)

$CN_MESSAGES = @(
    "开始安装...",
    "检测到架构：",
    "仅支持64位Windows系统",
    "最新版本：",
    "正在创建安装目录...",
    "正在从以下地址下载最新版本：",
    "下载二进制文件失败：",
    "未找到下载的文件",
    "正在安装程序...",
    "安装二进制文件失败：",
    "正在添加到PATH...",
    "正在清理...",
    "安装成功完成！",
    "现在可以直接使用 'flash-cat' 了"
)

# Detect system language
function Get-SystemLanguage {
    if ((Get-Culture).Name -like "zh-CN") {
        return "cn"
    }
    return "en"
}

# Get message based on language
function Get-Message($Index) {
    $lang = Get-SystemLanguage
    if ($lang -eq "cn") {
        return $CN_MESSAGES[$Index]
    }
    return $EN_MESSAGES[$Index]
}

# Functions for colored output
function Write-Status($Message) {
    Write-Host "${Blue}[*]${Reset} $Message"
}

function Write-Success($Message) {
    Write-Host "${Green}[✓]${Reset} $Message"
}

function Write-Warning($Message) {
    Write-Host "${Yellow}[!]${Reset} $Message"
}

function Write-Error($Message) {
    Write-Host "${Red}[✗]${Reset} $Message"
    Exit 1
}

# Get latest release version from GitHub
function Get-LatestVersion {
    $repo = "yunis-du/flash-cat"
    $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$repo/releases/latest"
    return $release.tag_name
}

# Add logging function at the beginning of the file
function Write-Log {
    param(
        [string]$Message,
        [string]$Level = "INFO"
    )
    $timestamp = Get-Date -Format "yyyy-MM-dd HH:mm:ss"
    $logMessage = "[$timestamp] [$Level] $Message"
    $logFile = "$env:TEMP\flash-cat-install.log"
    Add-Content -Path $logFile -Value $logMessage
    
    # Output to console
    switch ($Level) {
        "ERROR" { Write-Error $Message }
        "WARNING" { Write-Warning $Message }
        "SUCCESS" { Write-Success $Message }
        default { Write-Status $Message }
    }
}

# Add installation pre-check function
function Test-Prerequisites {
    Write-Log "Checking prerequisites..." "INFO"
    
    # Check PowerShell version
    if ($PSVersionTable.PSVersion.Major -lt 5) {
        Write-Log "PowerShell 5.0 or higher is required" "ERROR"
        return $false
    }
    
    # Check internet connection
    try {
        $testConnection = Test-Connection -ComputerName "github.com" -Count 1 -Quiet
        if (-not $testConnection) {
            Write-Log "No internet connection available" "ERROR"
            return $false
        }
    } catch {
        Write-Log "Failed to check internet connection: $_" "ERROR"
        return $false
    }
    
    return $true
}

# Add file verification function
function Test-FileHash {
    param(
        [string]$FilePath,
        [string]$ExpectedHash
    )
    
    $actualHash = Get-FileHash -Path $FilePath -Algorithm SHA256
    return $actualHash.Hash -eq $ExpectedHash
}

# Modify download function, add progress bar
function Download-File {
    param(
        [string]$Url,
        [string]$OutFile
    )
    
    try {
        $webClient = New-Object System.Net.WebClient
        $webClient.Headers.Add("User-Agent", "PowerShell Script")
        
        $webClient.DownloadFileAsync($Url, $OutFile)
        
        while ($webClient.IsBusy) {
            Write-Progress -Activity "Downloading..." -Status "Progress:" -PercentComplete -1
            Start-Sleep -Milliseconds 100
        }
        
        Write-Progress -Activity "Downloading..." -Completed
        return $true
    }
    catch {
        Write-Log "Download failed: $_" "ERROR"
        return $false
    }
    finally {
        if ($webClient) {
            $webClient.Dispose()
        }
    }
}

# Main installation process
Write-Status (Get-Message 0)

# Get system architecture
$arch = if ([Environment]::Is64BitOperatingSystem) { "amd64" } else { "386" }
Write-Status "$(Get-Message 1) $arch"

if ($arch -ne "amd64") {
    Write-Error (Get-Message 2)
}

# Get latest version
$version = Get-LatestVersion
Write-Status "$(Get-Message 3) $version"

# Set up paths
$installDir = "$env:ProgramFiles\flash-cat"
$versionWithoutV = $version.TrimStart('v')  # Remove 'v' prefix from version
$binaryName = "flash-cat-cli-windows-${versionWithoutV}-x86_64.zip"
$downloadUrl = "https://github.com/yunis-du/flash-cat/releases/download/$version/$binaryName"
$tempFile = "$env:TEMP\$binaryName"

# Create installation directory
Write-Status (Get-Message 4)
if (-not (Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

# Download binary
Write-Status "$(Get-Message 5) $downloadUrl"
try {
    if (-not (Download-File -Url $downloadUrl -OutFile $tempFile)) {
        Write-Error "$(Get-Message 6)"
    }
} catch {
    Write-Error "$(Get-Message 6) $_"
}

# Verify download
if (-not (Test-Path $tempFile)) {
    Write-Error (Get-Message 7)
}

# Install binary
Write-Status (Get-Message 8)
try {
    # Use .NET System.IO.Compression to unzip the file
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $tempExtractPath = "$env:TEMP\flash-cat-extract"
    
    # Clean up existing temporary directory
    if (Test-Path $tempExtractPath) {
        Remove-Item -Path $tempExtractPath -Recurse -Force
    }
    New-Item -ItemType Directory -Path $tempExtractPath -Force | Out-Null

    # Use .NET class to unzip the file
    [System.IO.Compression.ZipFile]::ExtractToDirectory($tempFile, $tempExtractPath)
    
    $extractedBinary = Get-ChildItem -Path $tempExtractPath -Filter "flash-cat.exe" -Recurse | Select-Object -First 1
    
    if (-not $extractedBinary) {
        Write-Error (Get-Message 9)
    }
    
    # Copy to installation directory
    Copy-Item -Path $extractedBinary.FullName -Destination "$installDir\flash-cat.exe" -Force
    
    # Clean up temporary files
    Remove-Item -Path $tempExtractPath -Recurse -Force
} catch {
    Write-Error "$(Get-Message 9) $_"
}

# Add to PATH if not already present
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$installDir*") {
    Write-Status (Get-Message 10)
    [Environment]::SetEnvironmentVariable(
        "Path",
        "$userPath;$installDir",
        "User"
    )
}

# Cleanup
Write-Status (Get-Message 11)
if (Test-Path $tempFile) {
    Remove-Item -Force $tempFile
}

Write-Success (Get-Message 12)
Write-Success (Get-Message 13)
Write-Host ""

# Run program directly
try {
    Start-Process -FilePath "$installDir\flash-cat.exe" -ArgumentList "-v" -NoNewWindow
} catch {
    Write-Warning "Failed to start flash-cat: $_"
}