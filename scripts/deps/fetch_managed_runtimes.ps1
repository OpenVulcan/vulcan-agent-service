param(
    # Target selects which managed runtime group to fetch.
    # Target 选择需要拉取的受管运行时分组。
    [ValidateSet("all", "python", "node", "package-managers")]
    [string]$Target = "all",
    # RuntimeRoot selects the LuaSkills/build root used for fixed defaults and staging.
    # RuntimeRoot 选择用于固定默认布局与暂存的 LuaSkills/构建根。
    [string]$RuntimeRoot = "third_party\managed_runtime_cache",
    # DistributionRoot optionally targets the directory that directly contains python and node.
    # DistributionRoot 可选指定直接包含 python 与 node 的目录。
    [string]$DistributionRoot = "third_party\luaskills_managed_runtimes",
    # PythonVersion selects the managed CPython version installed through uv.
    # PythonVersion 选择通过 uv 安装的受管 CPython 版本。
    [string]$PythonVersion = "3.14.6",
    # UvVersion selects the standalone uv binary version.
    # UvVersion 选择独立 uv 二进制版本。
    [string]$UvVersion = "0.11.28",
    # NodeVersion selects the managed Node.js version.
    # NodeVersion 选择受管 Node.js 版本。
    [string]$NodeVersion = "24.18.0",
    # PnpmVersion selects the pnpm package-manager version.
    # PnpmVersion 选择 pnpm 包管理器版本。
    [string]$PnpmVersion = "11.11.0",
    # Force removes existing targets before reinstalling them.
    # Force 会在重新安装前删除已有目标目录。
    [switch]$Force
)

$ErrorActionPreference = "Stop"

function Resolve-ProjectRoot {
    <#
    .SYNOPSIS
    Resolve the repository or packaged demo root from script metadata or the caller location.
    从脚本元数据或调用方位置解析仓库根目录或已发布 demo 包根目录。

    .PARAMETER ScriptDirectory
    Directory that contains the current script when PowerShell exposes it.
    PowerShell 可用时当前脚本所在的目录。

    .OUTPUTS
    Root path that contains Cargo.toml plus scripts, or packaged demo scripts.
    包含 Cargo.toml 与 scripts 的仓库根路径，或包含发布 demo 脚本的包根路径。
    #>
    param([string]$ScriptDirectory)

    # Candidates contains possible roots derived from script and caller locations.
    # Candidates 保存从脚本位置和调用方位置推导出的候选根目录。
    $Candidates = @()
    if ($ScriptDirectory) {
        $Candidates += $ScriptDirectory
    }
    $Candidates += (Get-Location).Path

    foreach ($Candidate in $Candidates) {
        # Current walks upward until a repository or packaged runtime root is found.
        # Current 向上遍历，直到找到仓库或已发布运行时根目录。
        $Current = $Candidate
        while ($Current) {
            if ((Test-Path -LiteralPath (Join-Path $Current "Cargo.toml")) -and (Test-Path -LiteralPath (Join-Path $Current "scripts"))) {
                return $Current
            }
            if (Test-Path -LiteralPath (Join-Path $Current "scripts\deps\fetch_managed_runtimes.ps1")) {
                return $Current
            }
            # Parent is the next directory to inspect.
            # Parent 是下一层需要检查的目录。
            $Parent = Split-Path -Parent $Current
            if (-not $Parent -or $Parent -eq $Current) {
                break
            }
            $Current = $Parent
        }
    }

    throw "Unable to resolve project root from script or current directory."
}

function Ensure-Dir {
    <#
    .SYNOPSIS
    Create one directory when it does not exist.
    在目录不存在时创建该目录。

    .PARAMETER Path
    Directory path to create.
    需要创建的目录路径。
    #>
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        New-Item -ItemType Directory -Path $Path -Force | Out-Null
    }
}

function Get-ManagedRuntimePlatform {
    <#
    .SYNOPSIS
    Resolve platform metadata used by managed runtime binary assets.
    解析受管运行时二进制资产使用的平台元数据。

    .OUTPUTS
    Hashtable containing platform key and upstream target fragments.
    包含平台键与上游目标片段的哈希表。
    #>
    # Arch stores the current process architecture in a normalized form.
    # Arch 保存当前进程架构的规范化形式。
    $Arch = [System.Runtime.InteropServices.RuntimeInformation]::ProcessArchitecture.ToString().ToLowerInvariant()
    switch ($Arch) {
        "x64" {
            $ArchKey = "x64"
            $RustArch = "x86_64"
            $NodeArch = "x64"
        }
        "arm64" {
            $ArchKey = "arm64"
            $RustArch = "aarch64"
            $NodeArch = "arm64"
        }
        default { throw "Unsupported architecture: $Arch" }
    }

    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Windows)) {
        if ($ArchKey -ne "x64") {
            throw "Managed runtime assets currently support Windows x64 only."
        }
        return @{
            key = "windows-x64"
            uv_asset = "uv-x86_64-pc-windows-msvc.zip"
            node_asset = "node-v{0}-win-x64.zip"
            node_extract_name = "node-v{0}-win-x64"
        }
    }
    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::OSX)) {
        return @{
            key = "macos-$ArchKey"
            uv_asset = "uv-$RustArch-apple-darwin.tar.gz"
            node_asset = "node-v{0}-darwin-$NodeArch.tar.gz"
            node_extract_name = "node-v{0}-darwin-$NodeArch"
        }
    }
    if ([System.Runtime.InteropServices.RuntimeInformation]::IsOSPlatform([System.Runtime.InteropServices.OSPlatform]::Linux)) {
        return @{
            key = "linux-$ArchKey"
            uv_asset = "uv-$RustArch-unknown-linux-gnu.tar.gz"
            node_asset = "node-v{0}-linux-$NodeArch.tar.xz"
            node_extract_name = "node-v{0}-linux-$NodeArch"
        }
    }
    throw "Unsupported operating system."
}

function Get-Sha256File {
    <#
    .SYNOPSIS
    Compute one file SHA-256 digest as lowercase hex.
    将单个文件计算为小写十六进制 SHA-256 摘要。

    .PARAMETER Path
    File path to hash.
    需要计算摘要的文件路径。
    #>
    param([string]$Path)

    # Stream keeps checksum verification available on Windows PowerShell hosts without Get-FileHash.
    # Stream 使缺少 Get-FileHash 的 Windows PowerShell 宿主仍可执行校验和验证。
    $Stream = [System.IO.File]::OpenRead((Resolve-Path -LiteralPath $Path).Path)
    try {
        # Hasher computes the exact archive digest used by upstream checksum manifests.
        # Hasher 计算上游校验和清单使用的精确归档摘要。
        $Hasher = [System.Security.Cryptography.SHA256]::Create()
        try {
            return ([System.BitConverter]::ToString($Hasher.ComputeHash($Stream))).Replace("-", "").ToLowerInvariant()
        }
        finally {
            $Hasher.Dispose()
        }
    }
    finally {
        $Stream.Dispose()
    }
}

function Get-Sha512Base64File {
    <#
    .SYNOPSIS
    Compute one file SHA-512 digest as base64 for npm integrity checks.
    将单个文件计算为用于 npm integrity 校验的 SHA-512 Base64 摘要。

    .PARAMETER Path
    File path to hash.
    需要计算摘要的文件路径。
    #>
    param([string]$Path)

    # Stream opens the file without loading large archives into memory.
    # Stream 以流方式打开文件，避免把大型归档整体读入内存。
    $Stream = [System.IO.File]::OpenRead((Resolve-Path -LiteralPath $Path).Path)
    try {
        # Sha512 is the hasher used by npm registry integrity metadata.
        # Sha512 是 npm registry integrity 元数据使用的哈希器。
        $Sha512 = [System.Security.Cryptography.SHA512]::Create()
        try {
            return [Convert]::ToBase64String($Sha512.ComputeHash($Stream))
        }
        finally {
            $Sha512.Dispose()
        }
    }
    finally {
        $Stream.Dispose()
    }
}

function Save-Url {
    <#
    .SYNOPSIS
    Download one URL to a local path.
    将一个 URL 下载到本地路径。

    .PARAMETER Url
    Source URL to download.
    需要下载的来源 URL。

    .PARAMETER Destination
    Local file path that receives the response body.
    接收响应体的本地文件路径。
    #>
    param(
        [string]$Url,
        [string]$Destination
    )

    Ensure-Dir (Split-Path -Parent $Destination)
    Invoke-WebRequest -Uri $Url -OutFile $Destination -UseBasicParsing
}

function Expand-ArchivePayload {
    <#
    .SYNOPSIS
    Extract one zip, tar.gz, or tar.xz archive into a destination directory.
    将 zip、tar.gz 或 tar.xz 归档解压到目标目录。

    .PARAMETER Archive
    Archive file to extract.
    需要解压的归档文件。

    .PARAMETER Destination
    Destination directory for extracted files.
    解压文件的目标目录。
    #>
    param(
        [string]$Archive,
        [string]$Destination
    )

    Ensure-Dir $Destination
    if ($Archive.EndsWith(".zip", [StringComparison]::OrdinalIgnoreCase)) {
        # bsdtar handles zip archives without Expand-Archive cleanup races or .NET long-path limits.
        # bsdtar 可处理 zip 归档，并避开 Expand-Archive 清理竞态和 .NET 长路径限制。
        if (Test-Path -LiteralPath $Destination) {
            Remove-Item -Recurse -Force -LiteralPath $Destination
        }
        Ensure-Dir $Destination
        & tar -xf $Archive -C $Destination
        if ($LASTEXITCODE -ne 0) {
            throw "Failed to extract archive: $Archive"
        }
        return
    }
    & tar -xf $Archive -C $Destination
    if ($LASTEXITCODE -ne 0) {
        throw "Failed to extract archive: $Archive"
    }
}

function Write-RuntimeManifest {
    <#
    .SYNOPSIS
    Write one stable runtime-manifest.json file.
    写入一个稳定的 runtime-manifest.json 文件。

    .PARAMETER Directory
    Directory that receives the manifest.
    接收 manifest 的目录。

    .PARAMETER Manifest
    Manifest object to serialize.
    需要序列化的 manifest 对象。
    #>
    param(
        [string]$Directory,
        [hashtable]$Manifest
    )

    Ensure-Dir $Directory
    $ManifestPath = Join-Path $Directory "runtime-manifest.json"
    # Utf8NoBom writes JSON that serde_json can parse consistently across PowerShell versions.
    # Utf8NoBom 写出 serde_json 在不同 PowerShell 版本下都能稳定解析的 JSON。
    $Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
    [System.IO.File]::WriteAllText($ManifestPath, (($Manifest | ConvertTo-Json -Depth 8) + [Environment]::NewLine), $Utf8NoBom)
}

function Get-RelativePathCompat {
    <#
    .SYNOPSIS
    Return one relative path without depending on newer .NET Path APIs.
    返回一个相对路径，且不依赖较新的 .NET Path API。

    .PARAMETER BasePath
    Base directory used as the relative path root.
    作为相对路径根的基础目录。

    .PARAMETER TargetPath
    Target path to render relative to the base directory.
    需要按基础目录渲染的目标路径。
    #>
    param(
        [string]$BasePath,
        [string]$TargetPath
    )

    # BaseFullPath stores a rooted directory path with a trailing separator for URI relativization.
    # BaseFullPath 保存带尾部分隔符的绝对目录路径，用于 URI 相对化。
    $BaseFullPath = (Resolve-Path -LiteralPath $BasePath).Path
    if (-not $BaseFullPath.EndsWith([System.IO.Path]::DirectorySeparatorChar)) {
        $BaseFullPath = $BaseFullPath + [System.IO.Path]::DirectorySeparatorChar
    }
    # TargetFullPath stores the rooted target path.
    # TargetFullPath 保存绝对目标路径。
    $TargetFullPath = (Resolve-Path -LiteralPath $TargetPath).Path
    # RelativeUri stores the URI-relative path between base and target.
    # RelativeUri 保存基础路径与目标路径之间的 URI 相对路径。
    $RelativeUri = ([Uri]$BaseFullPath).MakeRelativeUri([Uri]$TargetFullPath)
    return [Uri]::UnescapeDataString($RelativeUri.ToString()).Replace('/', [System.IO.Path]::DirectorySeparatorChar)
}

function Install-UvRuntime {
    <#
    .SYNOPSIS
    Download and install one standalone uv binary into the runtime root.
    下载并安装一个独立 uv 二进制到运行时根目录。

    .PARAMETER Platform
    Managed runtime platform metadata.
    受管运行时平台元数据。
    #>
    param([hashtable]$Platform)

    # UvTarget stores the versioned uv installation directory.
    # UvTarget 保存带版本的 uv 安装目录。
    $UvTarget = Join-Path $DistributionRoot "python\uv-$UvVersion-$($Platform.key)"
    # UvExeName stores the platform-specific uv executable name.
    # UvExeName 保存平台对应的 uv 可执行文件名。
    $UvExeName = if ($Platform.key.StartsWith("windows")) { "uv.exe" } else { "uv" }
    # UvExe stores the final executable path.
    # UvExe 保存最终可执行文件路径。
    $UvExe = Join-Path $UvTarget $UvExeName
    if ((Test-Path -LiteralPath $UvExe) -and -not $Force) {
        return $UvExe
    }
    if ((Test-Path -LiteralPath $UvTarget) -and $Force) {
        Remove-Item -Recurse -Force -LiteralPath $UvTarget
    }

    # AssetName stores the exact uv release asset name.
    # AssetName 保存精确的 uv release 资产名。
    $AssetName = $Platform.uv_asset
    # AssetUrl stores the official uv GitHub release asset URL.
    # AssetUrl 保存官方 uv GitHub release 资产地址。
    $AssetUrl = "https://github.com/astral-sh/uv/releases/download/$UvVersion/$AssetName"
    # ChecksumUrl stores the uv SHA-256 sidecar URL.
    # ChecksumUrl 保存 uv SHA-256 旁路校验文件地址。
    $ChecksumUrl = "$AssetUrl.sha256"
    # TempDir stores the temporary staging root.
    # TempDir 保存临时暂存根目录。
    $TempDir = Join-Path $RuntimeRoot "temp\managed-runtimes\uv-$UvVersion-$($Platform.key)"
    if (Test-Path -LiteralPath $TempDir) {
        Remove-Item -Recurse -Force -LiteralPath $TempDir
    }
    Ensure-Dir $TempDir

    # ArchivePath stores the downloaded uv archive.
    # ArchivePath 保存下载后的 uv 归档。
    $ArchivePath = Join-Path $TempDir $AssetName
    # ChecksumPath stores the downloaded uv checksum text.
    # ChecksumPath 保存下载后的 uv 校验文本。
    $ChecksumPath = Join-Path $TempDir "$AssetName.sha256"
    Save-Url $AssetUrl $ArchivePath
    Save-Url $ChecksumUrl $ChecksumPath

    # ExpectedHash stores the expected uv SHA-256 digest.
    # ExpectedHash 保存期望的 uv SHA-256 摘要。
    $ExpectedHash = ((Get-Content -LiteralPath $ChecksumPath -Raw).Trim() -split '\s+')[0].ToLowerInvariant()
    # ActualHash stores the actual uv archive digest.
    # ActualHash 保存 uv 归档的实际摘要。
    $ActualHash = Get-Sha256File $ArchivePath
    if ($ExpectedHash -ne $ActualHash) {
        throw "SHA-256 mismatch for $AssetName. Expected $ExpectedHash, got $ActualHash"
    }

    # ExtractDir stores the archive extraction directory.
    # ExtractDir 保存归档解压目录。
    $ExtractDir = Join-Path $TempDir "extract"
    Expand-ArchivePayload $ArchivePath $ExtractDir
    Ensure-Dir $UvTarget
    # ExtractedUv stores the first uv binary found in the archive payload.
    # ExtractedUv 保存归档中找到的第一个 uv 二进制文件。
    $ExtractedUv = Get-ChildItem -Path $ExtractDir -Recurse -File -Filter $UvExeName | Select-Object -First 1
    if (-not $ExtractedUv) {
        throw "uv executable '$UvExeName' not found in $AssetName"
    }
    Copy-Item -Force -LiteralPath $ExtractedUv.FullName -Destination $UvExe
    if (-not $Platform.key.StartsWith("windows")) {
        & chmod +x $UvExe
    }
    Write-RuntimeManifest $UvTarget @{
        schema_version = 1
        runtime = "uv"
        version = $UvVersion
        platform = $Platform.key
        executable = $UvExeName
        source = $AssetUrl
    }
    & $UvExe --version | Out-Host
    return $UvExe
}

function Install-PythonRuntime {
    <#
    .SYNOPSIS
    Install one managed CPython runtime through the managed uv binary.
    通过受管 uv 二进制安装一个受管 CPython 运行时。

    .PARAMETER Platform
    Managed runtime platform metadata.
    受管运行时平台元数据。
    #>
    param([hashtable]$Platform)

    # UvExe stores the managed uv executable used to fetch Python.
    # UvExe 保存用于拉取 Python 的受管 uv 可执行文件。
    $UvExe = Install-UvRuntime -Platform $Platform
    # PythonRoot stores uv-managed Python installations for this exact platform.
    # PythonRoot 保存当前平台下由 uv 管理的 Python 安装。
    $PythonRoot = Join-Path $DistributionRoot "python\cpython-$PythonVersion-$($Platform.key)"
    if ((Test-Path -LiteralPath (Join-Path $PythonRoot "runtime-manifest.json")) -and -not $Force) {
        return
    }
    if ((Test-Path -LiteralPath $PythonRoot) -and $Force) {
        Remove-Item -Recurse -Force -LiteralPath $PythonRoot
    }
    Ensure-Dir $PythonRoot

    # OldInstallDir stores the previous UV_PYTHON_INSTALL_DIR value for restoration.
    # OldInstallDir 保存旧的 UV_PYTHON_INSTALL_DIR 值以便恢复。
    $OldInstallDir = $env:UV_PYTHON_INSTALL_DIR
    try {
        $env:UV_PYTHON_INSTALL_DIR = $PythonRoot
        if ($Force) {
            & $UvExe python install $PythonVersion --reinstall
        } else {
            & $UvExe python install $PythonVersion
        }
        if ($LASTEXITCODE -ne 0) {
            throw "uv python install failed for Python $PythonVersion"
        }
        # PythonExe stores the managed interpreter path resolved by uv.
        # PythonExe 保存由 uv 解析出的受管解释器路径。
        $PythonExe = (& $UvExe python find $PythonVersion | Select-Object -First 1).Trim()
        if (-not $PythonExe -or -not (Test-Path -LiteralPath $PythonExe)) {
            throw "uv installed Python $PythonVersion but no interpreter path could be resolved"
        }
        # RelativeExe stores the interpreter path relative to PythonRoot when possible.
        # RelativeExe 在可行时保存相对 PythonRoot 的解释器路径。
        $RelativeExe = Get-RelativePathCompat -BasePath $PythonRoot -TargetPath $PythonExe
        Write-RuntimeManifest $PythonRoot @{
            schema_version = 1
            runtime = "python"
            version = $PythonVersion
            platform = $Platform.key
            executable = $RelativeExe
            source = "uv-managed-python"
            package_manager = "uv"
            package_manager_version = $UvVersion
        }
        & $PythonExe --version | Out-Host
    }
    finally {
        $env:UV_PYTHON_INSTALL_DIR = $OldInstallDir
    }
}

function Install-NodeRuntime {
    <#
    .SYNOPSIS
    Download and install one managed Node.js archive from nodejs.org.
    从 nodejs.org 下载并安装一个受管 Node.js 归档。

    .PARAMETER Platform
    Managed runtime platform metadata.
    受管运行时平台元数据。
    #>
    param([hashtable]$Platform)

    # NodeTarget stores the final versioned Node.js directory.
    # NodeTarget 保存最终带版本的 Node.js 目录。
    $NodeTarget = Join-Path $DistributionRoot "node\node-$NodeVersion-$($Platform.key)"
    # NodeExeName stores the platform-specific Node executable name.
    # NodeExeName 保存平台对应的 Node 可执行文件名。
    $NodeExeName = if ($Platform.key.StartsWith("windows")) { "node.exe" } else { "bin/node" }
    # NodeExe stores the final Node executable path.
    # NodeExe 保存最终 Node 可执行文件路径。
    $NodeExe = Join-Path $NodeTarget $NodeExeName
    if ((Test-Path -LiteralPath $NodeExe) -and -not $Force) {
        return $NodeExe
    }
    if ((Test-Path -LiteralPath $NodeTarget) -and $Force) {
        Remove-Item -Recurse -Force -LiteralPath $NodeTarget
    }

    # AssetName stores the exact Node.js archive name.
    # AssetName 保存精确的 Node.js 归档名。
    $AssetName = [string]::Format($Platform.node_asset, $NodeVersion)
    # ExtractName stores the expected top-level archive directory.
    # ExtractName 保存期望的归档顶层目录。
    $ExtractName = [string]::Format($Platform.node_extract_name, $NodeVersion)
    # BaseUrl stores the official Node.js release directory.
    # BaseUrl 保存官方 Node.js release 目录。
    $BaseUrl = "https://nodejs.org/dist/v$NodeVersion"
    # AssetUrl stores the exact Node.js archive URL.
    # AssetUrl 保存精确的 Node.js 归档地址。
    $AssetUrl = "$BaseUrl/$AssetName"
    # ShasumsUrl stores the official Node.js SHA-256 manifest URL.
    # ShasumsUrl 保存官方 Node.js SHA-256 清单地址。
    $ShasumsUrl = "$BaseUrl/SHASUMS256.txt"
    # TempDir stores the temporary staging root.
    # TempDir 保存临时暂存根目录。
    $TempDir = Join-Path $RuntimeRoot "temp\managed-runtimes\node-$NodeVersion-$($Platform.key)"
    if (Test-Path -LiteralPath $TempDir) {
        Remove-Item -Recurse -Force -LiteralPath $TempDir
    }
    Ensure-Dir $TempDir

    # ArchivePath stores the downloaded Node.js archive.
    # ArchivePath 保存下载后的 Node.js 归档。
    $ArchivePath = Join-Path $TempDir $AssetName
    # ShasumsPath stores the downloaded Node.js checksum manifest.
    # ShasumsPath 保存下载后的 Node.js 校验清单。
    $ShasumsPath = Join-Path $TempDir "SHASUMS256.txt"
    Save-Url $AssetUrl $ArchivePath
    Save-Url $ShasumsUrl $ShasumsPath

    # ExpectedLine stores the checksum manifest line for the target archive.
    # ExpectedLine 保存目标归档在校验清单中的行。
    # ShasumsText reads the manifest eagerly so older Windows PowerShell does not retain a pipeline file handle.
    # ShasumsText 预先完整读取清单，避免旧版 Windows PowerShell 保留管道文件句柄。
    $ShasumsText = [System.IO.File]::ReadAllText((Resolve-Path -LiteralPath $ShasumsPath).Path)
    # ExpectedLine selects the unique checksum row from the in-memory manifest.
    # ExpectedLine 从内存清单中选择唯一的校验和记录。
    $ExpectedLine = @($ShasumsText -split "`r?`n" | Where-Object { $_ -match "\s$([regex]::Escape($AssetName))$" })[0]
    if (-not $ExpectedLine) {
        throw "Checksum entry for $AssetName not found in SHASUMS256.txt"
    }
    # ExpectedHash stores the expected Node.js SHA-256 digest.
    # ExpectedHash 保存期望的 Node.js SHA-256 摘要。
    $ExpectedHash = ($ExpectedLine -split '\s+')[0].ToLowerInvariant()
    # ActualHash stores the actual Node.js archive digest.
    # ActualHash 保存 Node.js 归档的实际摘要。
    $ActualHash = Get-Sha256File $ArchivePath
    if ($ExpectedHash -ne $ActualHash) {
        throw "SHA-256 mismatch for $AssetName. Expected $ExpectedHash, got $ActualHash"
    }

    # ExtractDir stores the archive extraction directory.
    # ExtractDir 保存归档解压目录。
    $ExtractDir = Join-Path $TempDir "extract"
    Expand-ArchivePayload $ArchivePath $ExtractDir
    # ExtractedRoot stores the archive top-level payload directory.
    # ExtractedRoot 保存归档顶层载荷目录。
    $ExtractedRoot = Join-Path $ExtractDir $ExtractName
    if (-not (Test-Path -LiteralPath $ExtractedRoot)) {
        throw "Node archive root '$ExtractName' not found in $AssetName"
    }
    Ensure-Dir (Split-Path -Parent $NodeTarget)
    Move-Item -Force -LiteralPath $ExtractedRoot -Destination $NodeTarget
    Write-RuntimeManifest $NodeTarget @{
        schema_version = 1
        runtime = "node"
        version = $NodeVersion
        platform = $Platform.key
        executable = $NodeExeName
        source = $AssetUrl
    }
    & $NodeExe --version | Out-Host
    return $NodeExe
}

function Install-PnpmRuntime {
    <#
    .SYNOPSIS
    Download and install pnpm from npm registry without touching global npm state.
    从 npm registry 下载并安装 pnpm，且不触碰全局 npm 状态。

    .PARAMETER Platform
    Managed runtime platform metadata.
    受管运行时平台元数据。
    #>
    param([hashtable]$Platform)

    # NodeExe stores the managed Node executable used to run pnpm.
    # NodeExe 保存用于运行 pnpm 的受管 Node 可执行文件。
    $NodeExe = Install-NodeRuntime -Platform $Platform
    # PnpmTarget stores the versioned pnpm installation directory.
    # PnpmTarget 保存带版本的 pnpm 安装目录。
    $PnpmTarget = Join-Path $DistributionRoot "node\pnpm-$PnpmVersion"
    # PnpmEntry stores the pnpm CommonJS entry file.
    # PnpmEntry 保存 pnpm 的 CommonJS 入口文件。
    $PnpmEntry = Join-Path $PnpmTarget "bin\pnpm.cjs"
    if ((Test-Path -LiteralPath $PnpmEntry) -and -not $Force) {
        return
    }
    if ((Test-Path -LiteralPath $PnpmTarget) -and $Force) {
        Remove-Item -Recurse -Force -LiteralPath $PnpmTarget
    }

    # MetadataUrl stores the npm registry metadata URL for the exact pnpm version.
    # MetadataUrl 保存精确 pnpm 版本的 npm registry 元数据地址。
    $MetadataUrl = "https://registry.npmjs.org/pnpm/$PnpmVersion"
    # Metadata stores pnpm registry metadata.
    # Metadata 保存 pnpm registry 元数据。
    $Metadata = Invoke-RestMethod -Uri $MetadataUrl -UseBasicParsing
    # TarballUrl stores the pnpm tarball URL.
    # TarballUrl 保存 pnpm tarball 地址。
    $TarballUrl = [string]$Metadata.dist.tarball
    # Integrity stores the npm integrity digest.
    # Integrity 保存 npm integrity 摘要。
    $Integrity = [string]$Metadata.dist.integrity
    if (-not $TarballUrl -or -not $Integrity.StartsWith("sha512-")) {
        throw "pnpm metadata for $PnpmVersion does not contain a sha512 integrity tarball"
    }

    # TempDir stores the temporary staging root.
    # TempDir 保存临时暂存根目录。
    $TempDir = Join-Path $RuntimeRoot "temp\managed-runtimes\pnpm-$PnpmVersion"
    if (Test-Path -LiteralPath $TempDir) {
        Remove-Item -Recurse -Force -LiteralPath $TempDir
    }
    Ensure-Dir $TempDir
    # TarballPath stores the downloaded pnpm tarball.
    # TarballPath 保存下载后的 pnpm tarball。
    $TarballPath = Join-Path $TempDir "pnpm-$PnpmVersion.tgz"
    Save-Url $TarballUrl $TarballPath

    # ExpectedDigest stores the expected SHA-512 Base64 digest.
    # ExpectedDigest 保存期望的 SHA-512 Base64 摘要。
    $ExpectedDigest = $Integrity.Substring("sha512-".Length)
    # ActualDigest stores the actual SHA-512 Base64 digest.
    # ActualDigest 保存实际的 SHA-512 Base64 摘要。
    $ActualDigest = Get-Sha512Base64File $TarballPath
    if ($ExpectedDigest -ne $ActualDigest) {
        throw "SHA-512 integrity mismatch for pnpm $PnpmVersion"
    }

    # ExtractDir stores the pnpm extraction directory.
    # ExtractDir 保存 pnpm 解压目录。
    $ExtractDir = Join-Path $TempDir "extract"
    Expand-ArchivePayload $TarballPath $ExtractDir
    # PackageRoot stores the npm package root extracted from the tarball.
    # PackageRoot 保存从 tarball 解压出的 npm package 根目录。
    $PackageRoot = Join-Path $ExtractDir "package"
    if (-not (Test-Path -LiteralPath $PackageRoot)) {
        throw "pnpm package root not found in tarball"
    }
    Ensure-Dir (Split-Path -Parent $PnpmTarget)
    Move-Item -Force -LiteralPath $PackageRoot -Destination $PnpmTarget
    Write-RuntimeManifest $PnpmTarget @{
        schema_version = 1
        runtime = "pnpm"
        version = $PnpmVersion
        platform = "any"
        executable = "bin/pnpm.cjs"
        source = $TarballUrl
        node_runtime_version = $NodeVersion
    }
    & $NodeExe $PnpmEntry --version | Out-Host
}

# ScriptDir points at the current script directory when PowerShell exposes it.
# ScriptDir 在 PowerShell 提供脚本路径时指向当前脚本目录。
$ScriptDir = if ($PSScriptRoot) { $PSScriptRoot } elseif ($PSCommandPath) { Split-Path -Parent $PSCommandPath } elseif ($MyInvocation.MyCommand.Path) { Split-Path -Parent $MyInvocation.MyCommand.Path } else { "" }
# ProjectRoot points at the repository root regardless of the caller location.
# ProjectRoot 指向仓库根目录，避免调用方当前位置影响路径解析。
$ProjectRoot = Resolve-ProjectRoot -ScriptDirectory $ScriptDir
Set-Location $ProjectRoot
# RuntimeRoot is normalized after the project root has been resolved.
# RuntimeRoot 会在项目根目录解析后被规范化。
$RuntimeRoot = (New-Item -ItemType Directory -Force -Path $RuntimeRoot).FullName
# DistributionRoot defaults to the fixed runtime-root layout and otherwise uses the exact host path.
# DistributionRoot 默认使用固定的 runtime-root 布局，否则使用精确宿主路径。
if ([string]::IsNullOrWhiteSpace($DistributionRoot)) {
    $DistributionRoot = Join-Path $RuntimeRoot "dependencies\runtimes"
}
$DistributionRoot = (New-Item -ItemType Directory -Force -Path $DistributionRoot).FullName
# Platform stores the current managed runtime platform metadata.
# Platform 保存当前受管运行时平台元数据。
$Platform = Get-ManagedRuntimePlatform

switch ($Target) {
    "all" {
        Install-PythonRuntime -Platform $Platform
        Install-NodeRuntime -Platform $Platform | Out-Null
        Install-PnpmRuntime -Platform $Platform
    }
    "python" {
        Install-PythonRuntime -Platform $Platform
    }
    "node" {
        Install-NodeRuntime -Platform $Platform | Out-Null
        Install-PnpmRuntime -Platform $Platform
    }
    "package-managers" {
        Install-UvRuntime -Platform $Platform | Out-Null
        Install-PnpmRuntime -Platform $Platform
    }
}
