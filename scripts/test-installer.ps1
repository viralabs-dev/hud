# Testa o install.ps1 sem rede externa e sem tocar no PATH real do usuario: um
# servidor HTTP local (HttpListener) serve uma "release" montada numa pasta
# temporaria, e o instalador roda em modo de teste (HUD_INSTALLER_TEST=1, que
# libera HUD_DOWNLOAD_BASE=http://localhost:<porta> e o PATH do usuario num
# arquivo). E a mesma bateria do scripts/test-installer.sh.
#
# Uso:
#   pwsh -NoProfile -File scripts/test-installer.ps1 [-Package dist\hud_windows_amd64.zip]
#        [-Shell powershell.exe] [-Registry]
#   -Package   instala o pacote real (o hud.exe de verdade) como a release v0.2.0.
#   -Shell     qual PowerShell roda o instalador (padrao: o atual). No CI do
#              Windows roda com pwsh e com powershell.exe (5.1).
#   -Registry  so no Windows: tambem testa o PATH do usuario de verdade
#              (HKCU\Environment), restaurando o valor original no fim.
#
# Roda no Windows e no pwsh do Linux/macOS (Docker: mcr.microsoft.com/powershell).
# Fora do Windows o hud.exe falso e um script de shell, os links sao symlinks e
# o PATH do usuario e so o arquivo; o que depende do Windows (registro,
# junction NTFS, Get-FileHash no 5.1, hud.exe de verdade) so o runner
# windows-latest prova. Arquivo so com ASCII (o 5.1 le .ps1 sem BOM como ANSI).
param(
    [string]$Package = '',
    [string]$Shell = '',
    [switch]$Registry
)
Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
$install = Join-Path $root 'install.ps1'
$ehWindows = ($PSVersionTable.PSEdition -ne 'Core') -or ((Test-Path variable:IsWindows) -and $IsWindows)
if (-not $Shell) { $Shell = (Get-Process -Id $PID).Path }
if ($Package) { $Package = (Resolve-Path -LiteralPath $Package).ProviderPath }
$repo = 'teste/hud'
$asset = 'hud_windows_amd64.zip'
$script:pass = 0
$work = Join-Path ([IO.Path]::GetTempPath()) ('hud-test-ps-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $work | Out-Null
$sep = [IO.Path]::DirectorySeparatorChar

function Ok([string]$m) { $script:pass++; Write-Output ("ok {0} - {1}" -f $script:pass, $m) }
function Die([string]$m) {
    if (Test-Path -LiteralPath (Join-Path $work 'out')) {
        foreach ($l in Get-Content -LiteralPath (Join-Path $work 'out')) { [Console]::Error.WriteLine("  | $l") }
    }
    throw "FALHOU: $m"
}
function Sha([string]$p) { (Get-FileHash -LiteralPath $p -Algorithm SHA256).Hash.ToLowerInvariant() }
function Out-Text { Get-Content -Raw -LiteralPath (Join-Path $work 'out') }

# --- hud.exe falso -----------------------------------------------------------------
function New-FakeHud([string]$Versao, [string]$Destino) {
    if ($ehWindows) {
        $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
        if (-not (Test-Path -LiteralPath $csc)) { $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
        $src = Join-Path $work "fake-$Versao.cs"
        [IO.File]::WriteAllText($src, "class P { static int Main(string[] a) { System.Console.WriteLine(""hud $Versao""); return 0; } }")
        & $csc /nologo /target:exe "/out:$Destino" $src | Out-Null
        if ($LASTEXITCODE -ne 0) { throw 'csc falhou ao montar o hud.exe falso' }
    } else {
        [IO.File]::WriteAllText($Destino, "#!/bin/sh`necho ""hud $Versao""`n")
        & chmod 755 -- $Destino
    }
}

# --- pacotes --------------------------------------------------------------------------
Add-Type -AssemblyName System.IO.Compression
Add-Type -AssemblyName System.IO.Compression.FileSystem
function New-Zip([string]$Destino, [hashtable]$Membros) {
    # $Membros: nome no zip -> caminho local (ou $null para conteudo vazio)
    $fs = [IO.File]::Open($Destino, [IO.FileMode]::Create)
    $z = New-Object IO.Compression.ZipArchive($fs, [IO.Compression.ZipArchiveMode]::Create)
    try {
        foreach ($n in ($Membros.Keys | Sort-Object)) {
            $e = $z.CreateEntry($n)
            $s = $e.Open()
            try {
                if ($Membros[$n]) { $b = [IO.File]::ReadAllBytes($Membros[$n]); $s.Write($b, 0, $b.Length) }
            } finally { $s.Dispose() }
        }
    } finally { $z.Dispose(); $fs.Dispose() }
}
function Get-MembrosPadrao([string]$Hud) {
    $m = @{ 'hud.exe' = $Hud; 'LICENSE' = (Join-Path $root 'LICENSE'); 'THIRD_PARTY_NOTICES.md' = (Join-Path $root 'THIRD_PARTY_NOTICES.md') }
    foreach ($f in Get-ChildItem -LiteralPath (Join-Path $root 'LICENSES') -Filter '*.txt') { $m["LICENSES/$($f.Name)"] = $f.FullName }
    return $m
}
function Set-Checksums([string]$Dir) {
    $linhas = foreach ($f in Get-ChildItem -LiteralPath $Dir -Filter 'hud_*') { '{0}  {1}' -f (Sha $f.FullName), $f.Name }
    [IO.File]::WriteAllText((Join-Path $Dir 'checksums.txt'), (($linhas -join "`n") + "`n"))
}
$srv = Join-Path $work 'srv'
function Get-RelDir([string]$Tag) {
    $d = Join-Path $srv ("$repo/releases/download/$Tag".Replace('/', $sep))
    New-Item -ItemType Directory -Path $d -Force | Out-Null
    return $d
}
function New-Release([string]$Tag, [string]$Versao, [string]$Real = '') {
    $d = Get-RelDir $Tag
    if ($Real) {
        Copy-Item -LiteralPath $Real -Destination (Join-Path $d $asset)
    } else {
        $hud = Join-Path $work "hud-$Versao.exe"
        New-FakeHud $Versao $hud
        New-Zip (Join-Path $d $asset) (Get-MembrosPadrao $hud)
    }
    Set-Checksums $d
    return $d
}

New-Release 'v0.1.0' '0.1.0' | Out-Null
$v2 = New-Release 'v0.2.0' '0.2.0' $Package
$latestV = 'hud 0.2.0'
if ($Package) {
    $tmpx = Join-Path $work 'real'
    New-Item -ItemType Directory -Path $tmpx | Out-Null
    [IO.Compression.ZipFile]::ExtractToDirectory($Package, $tmpx)
    $p = Start-Process -FilePath (Join-Path $tmpx 'hud.exe') -ArgumentList '--version' -NoNewWindow -Wait -PassThru -RedirectStandardOutput (Join-Path $work 'realv')
    $latestV = (Get-Content -Raw -LiteralPath (Join-Path $work 'realv')).Trim()
}
$latestDir = Join-Path $srv ("$repo/releases/latest/download".Replace('/', $sep))
New-Item -ItemType Directory -Path $latestDir -Force | Out-Null
Copy-Item -Path (Join-Path $v2 '*') -Destination $latestDir
$v3 = Get-RelDir 'v0.3.0'
Copy-Item -LiteralPath (Join-Path $v2 $asset) -Destination $v3
[IO.File]::WriteAllText((Join-Path $v3 'checksums.txt'), ('0' * 64) + "  $asset`n")   # checksum errado
$v4 = Get-RelDir 'v0.4.0'
Copy-Item -LiteralPath (Join-Path $v2 $asset) -Destination $v4
[IO.File]::WriteAllText((Join-Path $v4 'checksums.txt'), "sem nada util`n")         # checksum ausente
# Pacotes maliciosos ou incompletos
$hudMal = Join-Path $work 'hud-mal.exe'
New-FakeHud '6.6.6' $hudMal
$i = 0
foreach ($ruim in '../fora.txt', 'LICENSES/../../fora.txt', '/abs.txt', 'C:/abs.txt', 'x:y.txt', '..\fora.txt') {
    $i++
    $m = Get-MembrosPadrao $hudMal
    $m[$ruim] = (Join-Path $root 'LICENSE')
    $d = Get-RelDir "v0.6.$i"
    New-Zip (Join-Path $d $asset) $m
    Set-Checksums $d
}
$maliciosos = $i
$m = Get-MembrosPadrao $hudMal
$m.Remove('hud.exe')
$d = Get-RelDir 'v0.7.0'
New-Zip (Join-Path $d $asset) $m
Set-Checksums $d
# Binario que nao roda: nao pode trocar o instalado. v0.8.0 nem e executavel (o
# Process.Start lanca excecao: "not a valid application for this OS platform" no
# Windows, "Exec format error" fora dele); v0.8.1 roda mas sai com erro.
$d = Get-RelDir 'v0.8.0'
$quebrado = Join-Path $work 'hud-quebrado.exe'
[IO.File]::WriteAllText($quebrado, 'isto nao e um executavel')
New-Zip (Join-Path $d $asset) (Get-MembrosPadrao $quebrado)
Set-Checksums $d
$d = Get-RelDir 'v0.8.1'
$saiErro = Join-Path $work 'hud-sai-erro.exe'
if ($ehWindows) {
    $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework64\v4.0.30319\csc.exe'
    if (-not (Test-Path -LiteralPath $csc)) { $csc = Join-Path $env:WINDIR 'Microsoft.NET\Framework\v4.0.30319\csc.exe' }
    $src = Join-Path $work 'sai-erro.cs'
    [IO.File]::WriteAllText($src, 'class P { static int Main() { System.Console.Error.WriteLine("falta uma DLL"); return 3; } }')
    & $csc /nologo /target:exe "/out:$saiErro" $src | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'csc falhou ao montar o hud.exe que sai com erro' }
} else {
    [IO.File]::WriteAllText($saiErro, "#!/bin/sh`necho 'falta uma DLL' >&2`nexit 3`n")
}
New-Zip (Join-Path $d $asset) (Get-MembrosPadrao $saiErro)
Set-Checksums $d
Copy-Item -LiteralPath $install -Destination (Join-Path $srv 'install.ps1')

# --- servidor HTTP local ------------------------------------------------------------
$serverPs1 = Join-Path $work 'server.ps1'
@'
param([string]$Root, [int]$Port, [string]$Log, [string]$Ready)
$l = New-Object Net.HttpListener
$l.Prefixes.Add("http://localhost:$Port/")
$l.Start()
[IO.File]::WriteAllText($Ready, 'ok')
while ($l.IsListening) {
    $c = $l.GetContext()
    $path = [Uri]::UnescapeDataString($c.Request.Url.AbsolutePath)
    [IO.File]::AppendAllText($Log, $c.Request.Url.AbsoluteUri + "`n")
    if ($path -eq '/__stop') { $c.Response.Close(); break }
    $f = Join-Path $Root $path.TrimStart('/')
    if ($path -notmatch '\.\.' -and [IO.File]::Exists($f)) {
        $b = [IO.File]::ReadAllBytes($f)
        $c.Response.ContentType = 'application/octet-stream'
        if ($f.EndsWith('.ps1')) { $c.Response.ContentType = 'text/plain; charset=utf-8' }
        $c.Response.ContentLength64 = $b.Length
        $c.Response.OutputStream.Write($b, 0, $b.Length)
    } else {
        $c.Response.StatusCode = 404
    }
    $c.Response.Close()
}
$l.Stop()
'@ | Set-Content -LiteralPath $serverPs1 -Encoding ASCII
$tl = New-Object Net.Sockets.TcpListener([Net.IPAddress]::Loopback, 0)
$tl.Start(); $port = $tl.LocalEndpoint.Port; $tl.Stop()
$log = Join-Path $work 'http.log'
$ready = Join-Path $work 'ready'
$server = Start-Process -FilePath (Get-Process -Id $PID).Path -PassThru -NoNewWindow `
    -ArgumentList @('-NoProfile', '-NonInteractive', '-ExecutionPolicy', 'Bypass', '-File', $serverPs1, $srv, $port, $log, $ready) `
    -RedirectStandardOutput (Join-Path $work 'server.out') -RedirectStandardError (Join-Path $work 'server.err')
for ($t = 0; $t -lt 100 -and -not (Test-Path -LiteralPath $ready); $t++) { Start-Sleep -Milliseconds 100 }
if (-not (Test-Path -LiteralPath $ready)) { throw 'servidor HTTP local nao subiu' }
$base = "http://localhost:$port"

# --- execucao do instalador ---------------------------------------------------------
$home_ = Join-Path $work 'home'
$dest = Join-Path $home_ ("AppData/Local/Programs/hud".Replace('/', $sep))
$pathFile = Join-Path $work 'user-path.txt'
[IO.File]::WriteAllText($pathFile, (('C:\Outro\bin', '%USERPROFILE%\bin') -join ';'))
$vars = 'HUD_VERSION', 'HUD_INSTALL_DIR', 'HUD_REPOSITORY', 'HUD_DOWNLOAD_BASE', 'HUD_INSTALLER_TEST', 'HUD_TEST_USER_PATH_FILE', 'LOCALAPPDATA'

function Invoke-Installer {
    # Invoke-Installer @{VAR = valor; ...} [-Iex] [-Uninstall] [-Raw]
    #   -Raw: sem os padroes de teste (HUD_INSTALLER_TEST etc.), so o que vier em $Env.
    param([hashtable]$Env = @{}, [switch]$Iex, [switch]$Uninstall, [switch]$Raw)
    $salvo = @{}
    foreach ($v in $vars) { $salvo[$v] = [Environment]::GetEnvironmentVariable($v) }
    try {
        foreach ($v in $vars) { [Environment]::SetEnvironmentVariable($v, $null) }
        if (-not $Raw) {
            $padrao = @{ HUD_INSTALLER_TEST = '1'; HUD_DOWNLOAD_BASE = $base; HUD_REPOSITORY = $repo
                         HUD_TEST_USER_PATH_FILE = $pathFile; HUD_INSTALL_DIR = $dest }
            foreach ($k in $padrao.Keys) { [Environment]::SetEnvironmentVariable($k, $padrao[$k]) }
        }
        foreach ($k in $Env.Keys) { [Environment]::SetEnvironmentVariable($k, $Env[$k]) }
        $out = Join-Path $work 'out'
        # No 5.1, stderr redirecionado de um programa vira erro e o Stop pararia o teste.
        $ErrorActionPreference = 'Continue'
        if ($Iex) {
            # Como o `irm .../install.ps1 | iex`, e o terminal continua depois de uma falha.
            $cmd = "irm '$base/install.ps1' | iex; Write-Output 'DEPOIS-DO-IEX'"
            & $Shell -NoProfile -NonInteractive -Command $cmd *> $out
        } elseif ($Uninstall) {
            & $Shell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $install -Uninstall *> $out
        } else {
            & $Shell -NoProfile -NonInteractive -ExecutionPolicy Bypass -File $install *> $out
        }
        return ($LASTEXITCODE -eq 0)
    } finally {
        foreach ($v in $vars) { [Environment]::SetEnvironmentVariable($v, $salvo[$v]) }
    }
}
function Get-Versao([string]$Exe) {
    $o = Join-Path $work 'ver'
    $p = Start-Process -FilePath $Exe -ArgumentList '--version' -NoNewWindow -Wait -PassThru -RedirectStandardOutput $o
    return (Get-Content -Raw -LiteralPath $o).Trim()
}
function Get-PathFileEntries { @(([IO.File]::ReadAllText($pathFile)) -split ';' | Where-Object { $_ }) }
function Test-SemTemporario([string]$Dir) {
    $sobra = @(Get-ChildItem -LiteralPath $Dir -Recurse -Force | Where-Object { $_.Name -like '.hud-install-*' -or $_.Name -like '.hud-tmp-*' })
    return $sobra.Count -eq 0
}
$hudExe = Join-Path $dest 'hud.exe'
$lic = Join-Path $dest 'hud-licenses'

try {
    # 0. o proprio instalador: so ASCII, sem BOM, e sem erro de sintaxe neste PowerShell
    $bytes = [IO.File]::ReadAllBytes($install)
    if (@($bytes | Where-Object { $_ -gt 127 }).Count -gt 0) { Die 'install.ps1 tem bytes fora do ASCII' }
    $tokens = $null; $erros = $null
    [Management.Automation.Language.Parser]::ParseFile($install, [ref]$tokens, [ref]$erros) | Out-Null
    if ($erros.Count -gt 0) { Die ("install.ps1 nao compila: " + ($erros | Out-String)) }
    if ((Get-Content -Raw -LiteralPath $install) -match 'Set-ExecutionPolicy\s') { Die 'install.ps1 chama Set-ExecutionPolicy' }
    Ok ("install.ps1 so ASCII e sintaxe valida ({0} {1})" -f $PSVersionTable.PSEdition, $PSVersionTable.PSVersion)

    # 1. latest
    if (-not (Invoke-Installer)) { Die 'instalacao latest' }
    if ((Get-Versao $hudExe) -ne $latestV) { Die "latest nao instalou $latestV" }
    if (-not (Test-Path -LiteralPath (Join-Path $lic 'THIRD_PARTY_NOTICES.md'))) { Die 'avisos nao instalados' }
    if ((Sha (Join-Path $root 'LICENSE')) -ne (Sha (Join-Path $lic 'LICENSE'))) { Die 'licenca MIT do HUD nao instalada' }
    foreach ($f in Get-ChildItem -LiteralPath (Join-Path $root 'LICENSES') -Filter '*.txt') {
        $inst = Join-Path $lic ("LICENSES/$($f.Name)".Replace('/', $sep))
        if (-not (Test-Path -LiteralPath $inst) -or (Sha $f.FullName) -ne (Sha $inst)) { Die "licenca ausente: $($f.Name)" }
    }
    if ((Get-Content -Raw -LiteralPath $log) -notmatch '/releases/latest/download/hud_windows_amd64\.zip') { Die 'latest nao usou releases/latest' }
    if ((Out-Text) -notmatch 'adicionado ao PATH') { Die 'nao avisou do PATH' }
    $e = Get-PathFileEntries
    if (@($e | Where-Object { $_ -eq $dest }).Count -ne 1 -or $e[0] -ne 'C:\Outro\bin' -or $e[1] -ne '%USERPROFILE%\bin') {
        Die "PATH do usuario errado: $($e -join ';')"
    }
    Ok 'instala a latest com binario, avisos de licenca e PATH do usuario'

    # 2. versao fixa (volta para a 0.1.0) e repeticao (atualiza para a 0.2.0)
    if (-not (Invoke-Installer @{ HUD_VERSION = 'v0.1.0' })) { Die 'instalacao v0.1.0' }
    if ((Get-Versao $hudExe) -ne 'hud 0.1.0') { Die 'HUD_VERSION=v0.1.0 nao instalou a 0.1.0' }
    if (-not (Invoke-Installer @{ HUD_VERSION = 'v0.2.0' })) { Die 'atualizacao' }
    if ((Get-Versao $hudExe) -ne $latestV) { Die 'repetir nao atualizou' }
    if (-not (Test-SemTemporario $dest)) { Die 'sobrou temporario' }
    if ((Out-Text) -notmatch 'PATH do usu') { Die 'nao disse que ja estava no PATH' }
    if (@(Get-PathFileEntries | Where-Object { $_ -eq $dest }).Count -ne 1) { Die 'PATH duplicado ao repetir' }
    Ok 'HUD_VERSION fixa e repeticao atualizam sem temporario e sem duplicar o PATH'

    # 3. HUD_INSTALL_DIR e PATH com a mesma pasta escrita de outro jeito (barra no fim, maiusculas)
    $outro = Join-Path $work 'outro'
    [IO.File]::WriteAllText($pathFile, ($outro.ToUpperInvariant() + $sep))
    if (-not (Invoke-Installer @{ HUD_INSTALL_DIR = $outro })) { Die 'HUD_INSTALL_DIR' }
    if (-not (Test-Path -LiteralPath (Join-Path $outro 'hud.exe')) -or -not (Test-Path -LiteralPath (Join-Path $outro 'hud-licenses/THIRD_PARTY_NOTICES.md'))) { Die 'HUD_INSTALL_DIR ignorado' }
    if (@(Get-PathFileEntries).Count -ne 1) { Die "duplicou o PATH: $(Get-PathFileEntries)" }
    if (Invoke-Installer @{ HUD_INSTALL_DIR = 'relativo\hud' }) { Die 'aceitou HUD_INSTALL_DIR relativo' }
    if ((Out-Text) -notmatch 'caminho absoluto') { Die 'mensagem de HUD_INSTALL_DIR relativo' }
    [IO.File]::WriteAllText($pathFile, (('C:\Outro\bin', '%USERPROFILE%\bin', $dest) -join ';'))
    Ok 'HUD_INSTALL_DIR (absoluto) e PATH sem duplicata mesmo com grafia diferente'

    # 4. checksum divergente e ausente: nao toca no binario instalado
    $antes = Sha $hudExe
    if (Invoke-Installer @{ HUD_VERSION = 'v0.3.0' }) { Die 'aceitou checksum divergente' }
    if ((Out-Text) -notmatch 'Checksum divergente') { Die 'mensagem de checksum divergente' }
    if (Invoke-Installer @{ HUD_VERSION = 'v0.4.0' }) { Die 'aceitou checksum ausente' }
    if ((Out-Text) -notmatch 'Checksum ausente') { Die 'mensagem de checksum ausente' }
    if ((Sha $hudExe) -ne $antes) { Die 'binario mudou apos checksum invalido' }
    Ok 'checksum divergente ou ausente interrompe sem mexer no instalado'

    # 5. versao ou repositorio invalidos e versao inexistente
    foreach ($v in '1.2.3', 'v1.2', 'v1.2.3;rm', '../v1.2.3', 'latest/../x', "v1.2.3`n") {
        if (Invoke-Installer @{ HUD_VERSION = $v }) { Die "aceitou HUD_VERSION=$v" }
        if ((Out-Text) -notmatch 'HUD_VERSION deve ser') { Die "mensagem para HUD_VERSION=$v" }
    }
    if (Invoke-Installer @{ HUD_VERSION = 'v9.9.9' }) { Die 'aceitou versao inexistente' }
    if ((Out-Text) -notmatch 'Falha ao baixar') { Die 'mensagem de versao inexistente' }
    foreach ($r in 'a/b/../c', 'semdono', 'a/b c', "a/b`n") {
        if (Invoke-Installer @{ HUD_REPOSITORY = $r }) { Die "aceitou HUD_REPOSITORY=$r" }
        if ((Out-Text) -notmatch 'HUD_REPOSITORY deve ser') { Die "mensagem para HUD_REPOSITORY=$r" }
    }
    Ok 'versao ou repositorio invalidos e versao inexistente sao recusados'

    # 6. so HTTPS fora do modo de teste; file:// nunca
    $semTeste = @{ HUD_DOWNLOAD_BASE = $base; HUD_REPOSITORY = $repo; HUD_INSTALL_DIR = $dest; HUD_TEST_USER_PATH_FILE = $pathFile }
    if ($ehWindows) {
        if (Invoke-Installer $semTeste -Raw) { Die 'aceitou http:// sem HUD_INSTALLER_TEST' }
        if ((Out-Text) -notmatch 'HUD_DOWNLOAD_BASE precisa ser https') { Die 'mensagem de http sem modo de teste' }
    } else {
        # Fora do Windows, sem o modo de teste o instalador ja para antes (e para o Windows).
        if (Invoke-Installer $semTeste -Raw) { Die 'rodou fora do Windows sem modo de teste' }
        if ((Out-Text) -notmatch 'para o Windows') { Die 'mensagem fora do Windows' }
    }
    foreach ($b in ('file:///' + $srv.Replace('\', '/')), 'http://example.com', 'ftp://localhost', "$base/../x", 'https://github.com/?x=1') {
        if (Invoke-Installer @{ HUD_DOWNLOAD_BASE = $b }) { Die "aceitou HUD_DOWNLOAD_BASE=$b" }
        if ((Out-Text) -notmatch 'HUD_DOWNLOAD_BASE precisa ser https') { Die "mensagem para HUD_DOWNLOAD_BASE=$b" }
    }
    Ok 'HUD_DOWNLOAD_BASE: so https:// fora do modo de teste; file://, http externo e lixo recusados'

    # 7. pacote com caminho suspeito, incompleto ou com binario que nao roda
    $antes = Sha $hudExe
    for ($i = 1; $i -le $maliciosos; $i++) {
        if (Invoke-Installer @{ HUD_VERSION = "v0.6.$i" }) { Die "aceitou pacote malicioso v0.6.$i" }
        if ((Out-Text) -notmatch 'caminho suspeito') { Die "mensagem do pacote malicioso v0.6.$i" }
    }
    if (Test-Path -LiteralPath (Join-Path $work 'fora.txt')) { Die 'extraiu fora da pasta' }
    if (Invoke-Installer @{ HUD_VERSION = 'v0.7.0' }) { Die 'aceitou pacote sem hud.exe' }
    if ((Out-Text) -notmatch 'Pacote incompleto') { Die 'mensagem do pacote incompleto' }
    if (Invoke-Installer @{ HUD_VERSION = 'v0.8.0' }) { Die 'aceitou binario que nao roda' }
    $o = Out-Text
    if ($o -notmatch 'n.{1,2}o roda nesta m.{1,2}quina') { Die 'mensagem do binario que nao roda' }
    if ($o -match 'Exception calling|Exce.{1,2}o ao chamar') { Die 'excecao crua do Process.Start no lugar da mensagem' }
    if ((Sha $hudExe) -ne $antes) { Die 'binario mudou apos binario que nao roda' }
    if (Invoke-Installer @{ HUD_VERSION = 'v0.8.1' }) { Die 'aceitou binario que sai com erro' }
    $o = Out-Text
    if ($o -notmatch 'n.{1,2}o roda nesta m.{1,2}quina' -or $o -notmatch 'falta uma DLL') { Die 'mensagem do binario que sai com erro' }
    if ((Sha $hudExe) -ne $antes) { Die 'binario mudou apos pacote ruim' }
    if (-not (Test-SemTemporario $dest)) { Die 'sobrou temporario apos pacote ruim' }
    Ok 'pacote com .., absoluto ou :, sem hud.exe ou com binario quebrado e recusado sem mexer no instalado'

    # 8. destino link (symlink ou junction): binario, pasta e avisos
    $real = Join-Path $work 'alvo-real'
    New-Item -ItemType Directory -Path $real | Out-Null
    $linkDir = Join-Path $work 'link-dir'
    if ($ehWindows) {
        New-Item -ItemType Junction -Path $linkDir -Target $real | Out-Null
    } else {
        New-Item -ItemType SymbolicLink -Path $linkDir -Target $real | Out-Null
    }
    if (Invoke-Installer @{ HUD_INSTALL_DIR = $linkDir }) { Die 'instalou numa pasta que e link' }
    if ((Out-Text) -notmatch 'link \(symlink ou junction\)') { Die 'mensagem da pasta link' }
    if (@(Get-ChildItem -LiteralPath $real -Force).Count -ne 0) { Die 'escreveu atraves do link da pasta' }
    $mv = Join-Path $work 'lic-real'
    Move-Item -LiteralPath $lic -Destination $mv
    if ($ehWindows) { New-Item -ItemType Junction -Path $lic -Target $mv | Out-Null } else { New-Item -ItemType SymbolicLink -Path $lic -Target $mv | Out-Null }
    if (Invoke-Installer) { Die 'escreveu avisos atraves de link' }
    if ((Out-Text) -notmatch 'link \(symlink ou junction\)') { Die 'mensagem dos avisos link' }
    if ($ehWindows) { [IO.Directory]::Delete($lic, $false) } else { Remove-Item -LiteralPath $lic -Force }
    Move-Item -LiteralPath $mv -Destination $lic
    $hudLinkOk = $false
    $velho = Join-Path $work 'hud-velho.exe'
    Copy-Item -LiteralPath $hudExe -Destination $velho
    $antes = Sha $velho
    Remove-Item -LiteralPath $hudExe -Force
    try {
        New-Item -ItemType SymbolicLink -Path $hudExe -Target $velho -ErrorAction Stop | Out-Null
        $hudLinkOk = $true
    } catch {
        Write-Output '   (symlink de arquivo exige modo de desenvolvedor ou administrador; caso do hud.exe link pulado)'
    }
    if ($hudLinkOk) {
        if (Invoke-Installer) { Die 'escreveu atraves do hud.exe link' }
        if ((Sha $velho) -ne $antes -or -not (Test-Path -LiteralPath $hudExe)) { Die 'mexeu no alvo do link' }
        Remove-Item -LiteralPath $hudExe -Force
    }
    if (-not (Invoke-Installer)) { Die 'instalacao depois de remover os links' }
    if ((Get-Versao $hudExe) -ne $latestV) { Die 'nao instalou depois de remover os links' }
    Ok 'destino link (pasta, avisos ou hud.exe) e recusado sem escrever atraves dele'

    # 9. irm | iex: instala, e uma falha nao fecha o terminal de quem rodou
    Remove-Item -LiteralPath $dest -Recurse -Force
    if (-not (Invoke-Installer -Iex)) { Die 'instalacao por irm | iex' }
    if ((Out-Text) -notmatch 'DEPOIS-DO-IEX') { Die 'irm | iex nao seguiu' }
    if ((Get-Versao $hudExe) -ne $latestV) { Die 'irm | iex nao instalou' }
    Invoke-Installer @{ HUD_VERSION = 'v0.3.0' } -Iex | Out-Null
    $o = Out-Text
    if ($o -notmatch 'Checksum divergente' -or $o -notmatch 'DEPOIS-DO-IEX') { Die 'falha no irm | iex fechou a sessao ou nao avisou' }
    Ok 'irm | iex instala, e uma falha nao encerra a sessao'

    # 10. desinstalar: tira hud.exe, avisos e a entrada do PATH; o resto fica
    $extra = Join-Path $dest 'meu-arquivo.txt'
    [IO.File]::WriteAllText($extra, 'nao apagar')
    if (-not (Invoke-Installer -Uninstall)) { Die 'desinstalar' }
    if ((Test-Path -LiteralPath $hudExe) -or (Test-Path -LiteralPath $lic)) { Die 'desinstalar deixou o HUD' }
    if (-not (Test-Path -LiteralPath $extra)) { Die 'desinstalar apagou arquivo alheio' }
    $e = Get-PathFileEntries
    if (@($e | Where-Object { $_ -eq $dest }).Count -ne 0 -or $e.Count -ne 2) { Die "desinstalar errou o PATH: $($e -join ';')" }
    Remove-Item -LiteralPath $extra
    if (-not (Invoke-Installer -Uninstall)) { Die 'desinstalar de novo' }
    if (Test-Path -LiteralPath $dest) { Die 'nao removeu a pasta vazia' }
    if ((Out-Text) -notmatch 'Nada para remover') { Die 'mensagem de nada para remover' }
    Ok 'desinstalar remove binario, avisos e PATH, e preserva o resto'

    # 11. elevado (administrador/root) sem HUD_INSTALL_DIR e recusado; comum instala no padrao
    $local = Join-Path $home_ 'AppData\Local'.Replace('\', $sep)
    $elevado = $false
    if ($ehWindows) {
        $elevado = (New-Object Security.Principal.WindowsPrincipal([Security.Principal.WindowsIdentity]::GetCurrent())).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } else {
        $elevado = ((& id -u) -eq '0')
    }
    $semDir = @{ HUD_INSTALL_DIR = ''; LOCALAPPDATA = $local }
    if ($elevado) {
        if (Invoke-Installer $semDir) { Die 'rodou elevado sem HUD_INSTALL_DIR' }
        if ((Out-Text) -notmatch 'administrador') { Die 'mensagem de elevado' }
        Ok 'elevado sem HUD_INSTALL_DIR e recusado (esta sessao e elevada)'
    } else {
        if (-not (Invoke-Installer $semDir)) { Die 'instalacao no padrao' }
        if (-not (Test-Path -LiteralPath (Join-Path $local ('Programs/hud/hud.exe'.Replace('/', $sep))))) { Die 'nao instalou em LOCALAPPDATA\Programs\hud' }
        Ok 'sem HUD_INSTALL_DIR instala em %LOCALAPPDATA%\Programs\hud (sessao comum)'
    }

    # 12. PATH do usuario de verdade (registro), so no Windows e com -Registry
    if ($Registry -and $ehWindows) {
        $k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
        $tinha = $k.GetValueNames() -contains 'Path'
        $orig = $null; $origTipo = $null
        if ($tinha) {
            $orig = $k.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            $origTipo = $k.GetValueKind('Path')
        }
        try {
            $k.SetValue('Path', '%USERPROFILE%\hud-teste-bin;C:\hud-teste-outro', [Microsoft.Win32.RegistryValueKind]::ExpandString)
            $regDir = Join-Path $work 'reg'
            $envReg = @{ HUD_INSTALL_DIR = $regDir; HUD_TEST_USER_PATH_FILE = '' }
            if (-not (Invoke-Installer $envReg)) { Die 'instalacao com o PATH no registro' }
            if (-not (Invoke-Installer $envReg)) { Die 'repeticao com o PATH no registro' }
            $v = $k.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            if ($k.GetValueKind('Path') -ne [Microsoft.Win32.RegistryValueKind]::ExpandString) { Die 'PATH do usuario perdeu o REG_EXPAND_SZ' }
            if ($v -ne "%USERPROFILE%\hud-teste-bin;C:\hud-teste-outro;$regDir") { Die "PATH do usuario no registro: $v" }
            $maquina = [Environment]::GetEnvironmentVariable('Path', 'Machine')
            if ($maquina -match [regex]::Escape($regDir)) { Die 'mexeu no PATH da maquina' }
            if (-not (Invoke-Installer $envReg -Uninstall)) { Die 'desinstalar com o PATH no registro' }
            $v = $k.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
            if ($v -ne '%USERPROFILE%\hud-teste-bin;C:\hud-teste-outro') { Die "desinstalar deixou o PATH: $v" }
            Ok 'PATH do usuario no registro: sem duplicata, REG_EXPAND_SZ preservado, PATH da maquina intacto'
        } finally {
            if ($tinha) { $k.SetValue('Path', $orig, $origTipo) } else { $k.DeleteValue('Path', $false) }
            $k.Close()
        }
    } elseif ($Registry) {
        Write-Output '   (-Registry ignorado: so no Windows)'
    }

    Write-Output ("instalador (PowerShell): {0} grupos de teste ok" -f $script:pass)
} finally {
    try { Invoke-WebRequest -Uri "$base/__stop" -UseBasicParsing -TimeoutSec 5 | Out-Null } catch { }
    if (-not $server.HasExited) { $server.WaitForExit(5000) | Out-Null }
    if (-not $server.HasExited) { $server.Kill() }
    Remove-Item -LiteralPath $work -Recurse -Force -ErrorAction SilentlyContinue
}
# Sucesso explicito: o "shell: pwsh" do GitHub Actions termina com exit $LASTEXITCODE,
# e o ultimo comando nativo (o instalador que devia falhar) pode ter saido com 1.
exit 0
