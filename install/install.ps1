# Installs cuilo from https://dl.cuilo.dev
#
#   irm https://dl.cuilo.dev/install.ps1 | iex
#   & ([scriptblock]::Create((irm https://dl.cuilo.dev/install.ps1))) -Version 2.0.0
#
# Environment:
#   CUILO_VERSION             the version (default: the newest stable release, or the newest
#                               pre-release while there is no stable one)
#   CUILO_INSTALL             the folder (default ~\.cuilo)
#   CUILO_NO_MODIFY_PATH=1    leave the user PATH alone
#   CUILO_REQUIRE_SIGNATURE=1 refuse to install unless minisign checks the signature
param(
	[string]$Version = $env:CUILO_VERSION
)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'
if ($PSVersionTable.PSVersion.Major -lt 6) {
	[Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
}

$DownloadUrl = 'https://dl.cuilo.dev'
$Repo = 'TuentyFaiv/cuilo'
$ReleaseKey = 'RWTFcgVmt+E7Xi77vi6Q/Yu68TJDTsBTKYHqOmyjSOY4VCgpEFCwl+dK'

# HTTPS only (the URLs above), no redirects.
function Get-File([string]$Uri, [string]$OutFile) {
	Invoke-WebRequest -UseBasicParsing -MaximumRedirection 0 -Uri $Uri -OutFile $OutFile
}

$machine = if ($env:PROCESSOR_ARCHITEW6432) { $env:PROCESSOR_ARCHITEW6432 } else { $env:PROCESSOR_ARCHITECTURE }
$arch = switch ($machine) {
	'AMD64' { 'x86_64' }
	'ARM64' { 'aarch64' }
	default { throw "cuilo has no Windows build for $machine" }
}
$target = "$arch-pc-windows-msvc"
$binary = "cuilo-$target.exe"
$installDir = if ($env:CUILO_INSTALL) { $env:CUILO_INSTALL } else { Join-Path $HOME '.cuilo' }
$bin = Join-Path $installDir 'bin'
$tmp = Join-Path ([IO.Path]::GetTempPath()) ([Guid]::NewGuid().ToString())
New-Item -ItemType Directory -Path $tmp | Out-Null

# The version a channel file names, or '' when the channel has no release yet.
function Get-Channel([string]$Name) {
	$file = Join-Path $tmp $Name
	try { Get-File "$DownloadUrl/$Name" $file } catch { return '' }
	return ((Get-Content -TotalCount 1 -Path $file) -as [string]).Trim()
}

try {
	if (-not $Version) {
		$Version = Get-Channel 'latest'
		if (-not $Version) {
			$Version = Get-Channel 'latest-pre'
			if (-not $Version) { throw "could not find the latest release at $DownloadUrl" }
			Write-Output "No stable release yet; installing the pre-release $Version."
		}
	}
	$Version = $Version.TrimStart('v')
	if ($Version -notmatch '^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$') { throw "``$Version`` is not a cuilo version" }
	$base = "$DownloadUrl/v$Version"

	$download = Join-Path $tmp $binary
	$sums = Join-Path $tmp 'SHA256SUMS'
	Get-File "$base/$binary" $download
	Get-File "$base/SHA256SUMS" $sums
	$expected = Get-Content $sums | ForEach-Object {
		$digest, $name = $_ -split '\s+', 2
		if ($name -and $name.TrimStart('*') -eq $binary) { $digest.ToLowerInvariant() }
	} | Select-Object -First 1
	if (-not $expected) { throw "SHA256SUMS does not list $binary" }
	$actual = (Get-FileHash -Algorithm SHA256 -Path $download).Hash.ToLowerInvariant()
	if ($actual -ne $expected) { throw "$binary does not match SHA256SUMS (expected $expected, got $actual)" }

	$minisign = Get-Command minisign -ErrorAction SilentlyContinue
	if ($ReleaseKey -and $minisign) {
		Get-File "$base/$binary.minisig" "$download.minisig"
		$signed = & $minisign.Source -V -Q -P $ReleaseKey -m $download 2>$null
		if ($LASTEXITCODE -ne 0) { throw "$binary is not signed by the cuilo release key" }
		if ("$signed".Trim() -ne "cuilo $Version $binary") {
			throw "$binary is signed as ``$signed``, not as cuilo $Version $binary"
		}
		$checked = 'signature and SHA-256 checked'
	} elseif ($env:CUILO_REQUIRE_SIGNATURE -eq '1') {
		if (-not $ReleaseKey) { throw 'this install script has no release key, so it cannot check the signature' }
		throw 'CUILO_REQUIRE_SIGNATURE=1 needs minisign (https://jedisct1.github.io/minisign/)'
	} else {
		$checked = 'SHA-256 checked; install minisign to check the signature too'
	}

	New-Item -ItemType Directory -Force -Path $bin | Out-Null
	$exe = Join-Path $bin 'cuilo.exe'
	# A running cuilo.exe cannot be overwritten, but it can be moved aside.
	if (Test-Path $exe) { Move-Item -Force -Path $exe -Destination (Join-Path $bin '.cuilo.exe.old') }
	Move-Item -Force -Path $download -Destination $exe
	$receipt = [ordered]@{ installer = 'install.ps1'; version = $Version; target = $target }
	$receipt | ConvertTo-Json | Set-Content -Encoding UTF8 -Path (Join-Path $bin 'cuilo-install.json')

	$installed = & $exe --version
	if ($LASTEXITCODE -ne 0) { throw 'the installed binary does not run' }
	Write-Output "Installed $installed to $bin ($checked)."

	$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
	$onPath = ($userPath -split ';') -contains $bin
	if (-not $onPath -and $env:CUILO_NO_MODIFY_PATH -ne '1') {
		$newPath = if ($userPath) { "$bin;$userPath" } else { $bin }
		[Environment]::SetEnvironmentVariable('Path', $newPath, 'User')
		Write-Output "Added $bin to your user PATH; open a new terminal to use cuilo."
	} elseif (-not $onPath) {
		Write-Output "Add $bin to your PATH to use cuilo."
	}
	Write-Output "Check where it was built with:`n  gh attestation verify $exe --repo $Repo"
} finally {
	Remove-Item -Recurse -Force -Path $tmp -ErrorAction SilentlyContinue
}
