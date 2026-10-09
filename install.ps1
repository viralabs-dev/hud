# Instalador do HUD para Windows (Windows PowerShell 5.1 e PowerShell 7+): baixa
# o binario autocontido da release (nao precisa de Python), confere o SHA-256 e
# instala hud.exe em %LOCALAPPDATA%\Programs\hud, com a pasta no PATH do usuario.
#
#   irm https://raw.githubusercontent.com/viralabs-dev/hud/main/install.ps1 | iex
#
# Para ler antes de rodar (modo arquivo):
#   irm https://raw.githubusercontent.com/viralabs-dev/hud/main/install.ps1 -OutFile install.ps1
#   notepad install.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\install.ps1 -Uninstall
# (o -ExecutionPolicy Bypass vale so para esse processo; o instalador nunca
# chama Set-ExecutionPolicy.) Desinstalar sem baixar o arquivo:
#   & ([scriptblock]::Create((irm https://raw.githubusercontent.com/viralabs-dev/hud/main/install.ps1))) -Uninstall
#
# Variaveis de ambiente:
#   HUD_VERSION      latest (padrao) ou uma tag vX.Y.Z
#   HUD_INSTALL_DIR  pasta de destino (padrao: %LOCALAPPDATA%\Programs\hud)
#   HUD_REPOSITORY   repositorio no GitHub (padrao: viralabs-dev/hud)
#
# Repetir a instalacao atualiza. Nao le nada do teclado, nao pede administrador
# (e recusa rodar elevado sem HUD_INSTALL_DIR), so usa HTTPS com TLS 1.2+, so
# mexe no PATH do usuario (nunca no da maquina) e recusa escrever atraves de
# symlink ou junction.
#
# So para testes (scripts/test-installer.ps1), com HUD_INSTALLER_TEST=1:
#   HUD_DOWNLOAD_BASE        no lugar de https://github.com; aceita tambem
#                            http://127.0.0.1:<porta> ou http://localhost:<porta>.
#                            Sem HUD_INSTALLER_TEST=1, so https:// e aceito.
#   HUD_TEST_USER_PATH_FILE  arquivo que faz o papel do PATH do usuario (fora do
#                            Windows, onde nao ha registro).
#   e o instalador roda fora do Windows (com um hud.exe falso).
#
# O arquivo so tem ASCII: o Windows PowerShell 5.1 le .ps1 sem BOM como ANSI, e
# um BOM quebraria o `irm | iex`. Os acentos das mensagens vem de \uXXXX.
param([switch]$Uninstall)

& {
param([bool]$Remover, [bool]$ModoArquivo)
Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'  # a barra de progresso do 5.1 deixa o download muito lento

function Msg([string]$Texto) {
    # Mensagem com \uXXXX trocado pelo caractere e {0}, {1}... pelos argumentos.
    $s = [regex]::Unescape($Texto)
    if ($args.Count -gt 0) { $s = $s -f $args }
    return $s
}

$teste = ($env:HUD_INSTALLER_TEST -eq '1')
$ehWindows = ($PSVersionTable.PSEdition -ne 'Core') -or ((Test-Path variable:IsWindows) -and $IsWindows)
$pathFile = $null
if ($teste -and $env:HUD_TEST_USER_PATH_FILE) { $pathFile = $env:HUD_TEST_USER_PATH_FILE }

function Get-Atributos([string]$Caminho) {
    # Atributos sem seguir link; $null se nao existe (um link quebrado existe).
    $fi = New-Object IO.FileInfo $Caminho
    $fi.Refresh()
    if ([int]$fi.Attributes -eq -1) { return $null }
    return $fi.Attributes
}

function Test-Reparse([string]$Caminho) {
    $a = Get-Atributos $Caminho
    return ($null -ne $a) -and (($a -band [IO.FileAttributes]::ReparsePoint) -ne 0)
}

function Assert-SemLink([string]$Caminho) {
    if (Test-Reparse $Caminho) {
        throw (Msg '{0} \u00e9 um link (symlink ou junction). O instalador n\u00e3o escreve atrav\u00e9s de link; remova-o ou escolha outro HUD_INSTALL_DIR.' $Caminho)
    }
}

function Test-UrlPermitida([Uri]$Uri) {
    if ($Uri.Scheme -eq 'https') { return $true }
    return $teste -and $Uri.Scheme -eq 'http' -and ($Uri.Host -eq '127.0.0.1' -or $Uri.Host -eq 'localhost')
}

function Get-Download([string]$Url, [string]$Destino) {
    for ($i = 1; $i -le 3; $i++) {
        $parar = $false
        try {
            $r = Invoke-WebRequest -Uri $Url -OutFile $Destino -UseBasicParsing -PassThru -MaximumRedirection 10 -TimeoutSec 300
            # Depois de redirecionamentos, a URL final tambem precisa ser HTTPS.
            $final = $null
            try { $final = $r.BaseResponse.ResponseUri } catch { $final = $null }                   # 5.1
            if (-not $final) {
                try { $final = $r.BaseResponse.RequestMessage.RequestUri } catch { $final = $null }  # 7+
            }
            if ($final -and -not (Test-UrlPermitida ([Uri]$final))) {
                $parar = $true
                throw (Msg 'Redirecionado para um endere\u00e7o sem HTTPS: {0}' $final)
            }
            return
        } catch {
            $status = 0
            try { $status = [int]$_.Exception.Response.StatusCode } catch { $status = 0 }
            if ($parar -or ($status -ge 400 -and $status -lt 500) -or $i -eq 3) {
                throw (Msg 'Falha ao baixar {0} ({1})' $Url $_.Exception.Message)
            }
            Start-Sleep -Seconds $i
        }
    }
}

function Invoke-Exe([string]$Exe, [string]$Argumentos) {
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $Exe
    $psi.Arguments = $Argumentos
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $psi.RedirectStandardInput = $true
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    try {
        $p = [Diagnostics.Process]::Start($psi)
    } catch {
        # Arquivo que nao e um executavel valido (ou bloqueado pelo antivirus): o
        # Windows recusa ja na partida, com Win32Exception dentro da excecao.
        $ex = $_.Exception
        while ($ex.InnerException) { $ex = $ex.InnerException }
        return [pscustomobject]@{ Code = -1; Out = $ex.Message.Trim() }
    }
    $p.StandardInput.Close()
    $o = $p.StandardOutput.ReadToEndAsync()
    $e = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit(120000)) {
        try { $p.Kill() } catch { }
        return [pscustomobject]@{ Code = -1; Out = 'tempo esgotado' }
    }
    $p.WaitForExit()
    return [pscustomobject]@{ Code = $p.ExitCode; Out = ($o.Result + $e.Result).Trim() }
}

function Move-Atomico([string]$Origem, [string]$Destino) {
    # Troca o destino de uma vez (ReplaceFile no Windows, rename no resto).
    Assert-SemLink $Destino
    if ($null -ne (Get-Atributos $Destino)) {
        [IO.File]::Replace($Origem, $Destino, [NullString]::Value)
    } else {
        [IO.File]::Move($Origem, $Destino)
    }
}

function Copy-Atomico([string]$Origem, [string]$Destino) {
    $tmp = Join-Path (Split-Path -Parent $Destino) ('.hud-tmp-' + [guid]::NewGuid().ToString('N'))
    try {
        [IO.File]::Copy($Origem, $tmp)
        Move-Atomico $tmp $Destino
    } finally {
        if ($null -ne (Get-Atributos $tmp)) { Remove-Item -LiteralPath $tmp -Force }
    }
}

# --- PATH do usuario -----------------------------------------------------------
# Lido e gravado direto em HKCU\Environment, preservando o tipo (REG_EXPAND_SZ):
# o [Environment]::GetEnvironmentVariable('Path','User') devolveria as entradas
# %USERPROFILE%\... ja expandidas e o Set as gravaria como REG_SZ.
function Get-PathUsuario {
    if ($pathFile) {
        if ($null -eq (Get-Atributos $pathFile)) { return '' }
        return ([IO.File]::ReadAllText($pathFile)).TrimEnd("`r", "`n")
    }
    if (-not $ehWindows) { throw 'HUD_TEST_USER_PATH_FILE e obrigatorio fora do Windows.' }
    $k = [Microsoft.Win32.Registry]::CurrentUser.OpenSubKey('Environment', $false)
    if ($null -eq $k) { return '' }
    try {
        return [string]$k.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
    } finally { $k.Close() }
}

function Set-PathUsuario([string]$Valor) {
    if ($pathFile) { [IO.File]::WriteAllText($pathFile, $Valor); return }
    $k = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
    try {
        $tipo = [Microsoft.Win32.RegistryValueKind]::ExpandString
        if ($k.GetValueNames() -contains 'Path') {
            $atual = $k.GetValueKind('Path')
            if ($atual -eq [Microsoft.Win32.RegistryValueKind]::String) { $tipo = $atual }
        }
        if ($Valor) { $k.SetValue('Path', $Valor, $tipo) } else { $k.DeleteValue('Path', $false) }
    } finally { $k.Close() }
    # Avisa os programas abertos (WM_SETTINGCHANGE), como o SetEnvironmentVariable
    # faz: grava e apaga uma variavel descartavel do usuario.
    $tmpVar = 'HUD_INSTALL_' + [guid]::NewGuid().ToString('N')
    [Environment]::SetEnvironmentVariable($tmpVar, '1', 'User')
    [Environment]::SetEnvironmentVariable($tmpVar, $null, 'User')
}

function Get-ChavePath([string]$Entrada) {
    $e = [Environment]::ExpandEnvironmentVariables($Entrada.Trim().Trim('"'))
    return $e.TrimEnd('\', '/').ToLowerInvariant()
}

function Get-EntradasPath([string]$Valor) {
    return @($Valor -split ';' | Where-Object { $_.Trim() -ne '' })
}

# --- validacoes ----------------------------------------------------------------
function Get-Config {
    if (-not $ehWindows -and -not $teste) {
        throw (Msg 'Este instalador \u00e9 para o Windows. No Linux e no macOS: curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash')
    }
    if ($ehWindows) {
        $arq = $env:PROCESSOR_ARCHITEW6432
        if (-not $arq) { $arq = $env:PROCESSOR_ARCHITECTURE }
        if ($arq -ne 'AMD64' -and $arq -ne 'ARM64') {
            throw (Msg 'Arquitetura n\u00e3o suportada: {0} (h\u00e1 bin\u00e1rio para Windows amd64).' $arq)
        }
    }
    $repo = $env:HUD_REPOSITORY
    if (-not $repo) { $repo = 'viralabs-dev/hud' }
    if ($repo -cnotmatch '\A[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+\z') { throw (Msg 'HUD_REPOSITORY deve ser dono/reposit\u00f3rio.') }
    $versao = $env:HUD_VERSION
    if (-not $versao) { $versao = 'latest' }
    if ($versao -cne 'latest' -and $versao -cnotmatch '\Av[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?\z') {
        throw (Msg 'HUD_VERSION deve ser latest ou uma tag vX.Y.Z.')
    }
    $base = 'https://github.com'
    if ($env:HUD_DOWNLOAD_BASE) {
        $b = $env:HUD_DOWNLOAD_BASE.TrimEnd('/')
        $local = $b -cmatch '\Ahttp://(127\.0\.0\.1|localhost)(:[0-9]{1,5})?\z'
        if ($b -cnotmatch '\Ahttps://[A-Za-z0-9.-]+(:[0-9]{1,5})?(/[A-Za-z0-9._~-]+)*\z' -and -not ($teste -and $local)) {
            throw (Msg 'HUD_DOWNLOAD_BASE precisa ser https:// (http://127.0.0.1 s\u00f3 com HUD_INSTALLER_TEST=1, nos testes).')
        }
        $base = $b
    }

    # Elevado: instalaria no perfil e no PATH do administrador, nao no do usuario.
    $elevado = $false
    if ($ehWindows) {
        $id = [Security.Principal.WindowsIdentity]::GetCurrent()
        $elevado = (New-Object Security.Principal.WindowsPrincipal $id).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)
    } else {
        $elevado = ((& id -u) -eq '0')
    }
    if ($elevado -and -not $env:HUD_INSTALL_DIR) {
        throw (Msg 'N\u00e3o rode o instalador como administrador: o HUD \u00e9 instalado para o seu usu\u00e1rio. Abra um PowerShell comum (sem "Executar como administrador") ou defina HUD_INSTALL_DIR.')
    }

    if ($env:HUD_INSTALL_DIR) {
        $dir = $env:HUD_INSTALL_DIR
        $absoluto = [IO.Path]::IsPathRooted($dir) -and ($dir -match '\A([A-Za-z]:[\\/]|\\\\|/)')
        if (-not $absoluto) { throw (Msg 'HUD_INSTALL_DIR precisa ser um caminho absoluto (ex.: C:\\Ferramentas\\hud).') }
    } else {
        $local = $env:LOCALAPPDATA
        if (-not $local) { $local = [Environment]::GetFolderPath('LocalApplicationData') }
        if (-not $local) { throw (Msg 'N\u00e3o achei a pasta LOCALAPPDATA; defina HUD_INSTALL_DIR.') }
        $dir = Join-Path (Join-Path $local 'Programs') 'hud'
    }
    $dir = [IO.Path]::GetFullPath($dir).TrimEnd('\', '/')

    return [pscustomobject]@{ Repo = $repo; Versao = $versao; Base = $base; Dir = $dir; Elevado = $elevado }
}

function Test-MembroSeguro([string]$Nome) {
    if (-not $Nome -or $Nome.Contains(':') -or $Nome -match '\A[\\/]' -or $Nome -match '[\x00-\x1f]') { return $false }
    foreach ($parte in ($Nome -split '[\\/]')) { if ($parte -eq '..') { return $false } }
    return $true
}

function Expand-Pacote([string]$Zip, [string]$Destino) {
    # So os membros esperados, como arquivos comuns; recusa caminho suspeito.
    Add-Type -AssemblyName System.IO.Compression
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $limite = 512MB
    $total = [long]0
    $vistos = @{}  # chaves sem distinguir maiusculas, como no NTFS
    $extraidos = New-Object Collections.Generic.List[string]
    $z = [IO.Compression.ZipFile]::OpenRead($Zip)
    try {
        foreach ($e in $z.Entries) {
            $n = $e.FullName
            if (-not (Test-MembroSeguro $n)) { throw (Msg 'Pacote recusado: caminho suspeito "{0}".' $n) }
            $n = $n.Replace('\', '/')
            if ($n.EndsWith('/')) { continue }
            if ($vistos.ContainsKey($n)) { throw (Msg 'Pacote recusado: membro repetido "{0}".' $n) }
            $vistos[$n] = $true
            if ($n -cnotmatch '\A(hud\.exe|LICENSE|THIRD_PARTY_NOTICES\.md|LICENSES/[A-Za-z0-9._-]+\.txt)\z') { continue }
            $alvo = Join-Path $Destino ($n.Replace('/', [IO.Path]::DirectorySeparatorChar))
            $pai = Split-Path -Parent $alvo
            if (-not (Test-Path -LiteralPath $pai)) { New-Item -ItemType Directory -Path $pai | Out-Null }
            $in = $e.Open()
            try {
                $out = New-Object IO.FileStream($alvo, [IO.FileMode]::CreateNew, [IO.FileAccess]::Write)
                try {
                    $buf = New-Object byte[] 65536
                    while (($lidos = $in.Read($buf, 0, $buf.Length)) -gt 0) {
                        $total += $lidos
                        if ($total -gt $limite) { throw (Msg 'Pacote recusado: maior que 512 MB descompactado.') }
                        $out.Write($buf, 0, $lidos)
                    }
                } finally { $out.Dispose() }
            } finally { $in.Dispose() }
            $extraidos.Add($n)
        }
    } finally { $z.Dispose() }
    foreach ($obrigatorio in 'hud.exe', 'LICENSE', 'THIRD_PARTY_NOTICES.md') {
        if (-not $extraidos.Contains($obrigatorio)) {
            throw (Msg 'Pacote incompleto: falta {0}.' $obrigatorio)
        }
    }
    return ,$extraidos.ToArray()
}

# --- instalar e desinstalar -----------------------------------------------------
function Install-Hud($Cfg) {
    $dir = $Cfg.Dir
    $alvo = Join-Path $dir 'hud.exe'
    $avisos = Join-Path $dir 'hud-licenses'
    $asset = 'hud_windows_amd64.zip'
    if ($Cfg.Versao -eq 'latest') {
        $url = '{0}/{1}/releases/latest/download' -f $Cfg.Base, $Cfg.Repo
    } else {
        $url = '{0}/{1}/releases/download/{2}' -f $Cfg.Base, $Cfg.Repo, $Cfg.Versao
    }

    # Destino: recusa link antes de baixar qualquer coisa.
    $attrDir = Get-Atributos $dir
    if ($null -ne $attrDir) {
        Assert-SemLink $dir
        if (($attrDir -band [IO.FileAttributes]::Directory) -eq 0) { throw (Msg '{0} existe e n\u00e3o \u00e9 uma pasta.' $dir) }
    }
    foreach ($p in $alvo, $avisos, (Join-Path $avisos 'LICENSES')) { Assert-SemLink $p }
    $attrAlvo = Get-Atributos $alvo
    if ($null -ne $attrAlvo -and ($attrAlvo -band [IO.FileAttributes]::Directory) -ne 0) {
        throw (Msg '{0} existe e n\u00e3o \u00e9 um arquivo comum.' $alvo)
    }

    $tmp = Join-Path ([IO.Path]::GetTempPath()) ('hud-install-' + [guid]::NewGuid().ToString('N'))
    New-Item -ItemType Directory -Path $tmp | Out-Null
    $novo = $null
    try {
        Write-Host (Msg 'Baixando {0} ({1})...' $asset $Cfg.Versao)
        $zip = Join-Path $tmp $asset
        $sums = Join-Path $tmp 'checksums.txt'
        Get-Download "$url/$asset" $zip
        Get-Download "$url/checksums.txt" $sums

        $esperados = @()
        foreach ($linha in [IO.File]::ReadAllLines($sums)) {
            $m = [regex]::Match($linha.TrimEnd("`r"), '\A(\S+) [ *]?(.+)\z')
            if ($m.Success -and $m.Groups[2].Value -ceq $asset) { $esperados += $m.Groups[1].Value }
        }
        if ($esperados.Count -ne 1 -or $esperados[0] -cnotmatch '\A[0-9a-f]{64}\z') {
            throw (Msg 'Checksum ausente ou inv\u00e1lido em checksums.txt.')
        }
        $atual = (Get-FileHash -LiteralPath $zip -Algorithm SHA256).Hash.ToLowerInvariant()
        if ($atual -cne $esperados[0]) { throw (Msg 'Checksum divergente; instala\u00e7\u00e3o interrompida.') }

        $x = Join-Path $tmp 'x'
        New-Item -ItemType Directory -Path $x | Out-Null
        $membros = Expand-Pacote $zip $x

        # Escrita atomica: copia para um temporario na mesma pasta, testa e troca.
        if ($null -eq (Get-Atributos $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }
        Assert-SemLink $dir
        $novo = Join-Path $dir ('.hud-install-' + [guid]::NewGuid().ToString('N') + '.exe')
        [IO.File]::Copy((Join-Path $x 'hud.exe'), $novo)
        if (-not $ehWindows) { & chmod 755 -- $novo }
        $r = Invoke-Exe $novo '--version'
        if ($r.Code -ne 0 -or $r.Out -notmatch '\Ahud \S+') {
            throw (Msg 'O bin\u00e1rio baixado n\u00e3o roda nesta m\u00e1quina (Windows 10 ou mais novo? antiv\u00edrus bloqueou?). Sa\u00edda: {0}' $r.Out)
        }
        try {
            Move-Atomico $novo $alvo
        } catch {
            throw (Msg 'N\u00e3o consegui trocar {0}: {1} Se o HUD estiver aberto, feche-o e rode o instalador de novo.' $alvo $_.Exception.Message)
        }
        $novo = $null

        $licDir = Join-Path $avisos 'LICENSES'
        foreach ($p in $avisos, $licDir) {
            if ($null -eq (Get-Atributos $p)) { New-Item -ItemType Directory -Path $p | Out-Null }
            Assert-SemLink $p
        }
        foreach ($m in $membros) {
            if ($m -ceq 'hud.exe') { continue }
            Copy-Atomico (Join-Path $x $m) (Join-Path $avisos ($m.Replace('/', [IO.Path]::DirectorySeparatorChar)))
        }
    } finally {
        if ($novo -and ($null -ne (Get-Atributos $novo))) { Remove-Item -LiteralPath $novo -Force }
        Remove-Item -LiteralPath $tmp -Recurse -Force -ErrorAction SilentlyContinue
    }

    $versao = (Invoke-Exe $alvo '--version').Out
    Write-Host (Msg 'HUD instalado: {0} ({1})' $alvo $versao)
    Write-Host (Msg 'Avisos de licen\u00e7a: {0}' $avisos)

    # PATH do usuario, sem duplicar; o da maquina nao e tocado.
    $chave = Get-ChavePath $dir
    $userPath = Get-PathUsuario
    $ja = @(Get-EntradasPath $userPath | Where-Object { (Get-ChavePath $_) -eq $chave }).Count -gt 0
    if ($ja) {
        Write-Host (Msg '{0} j\u00e1 est\u00e1 no PATH do usu\u00e1rio.' $dir)
    } else {
        $novoPath = (@(Get-EntradasPath $userPath) + $dir) -join ';'
        Set-PathUsuario $novoPath
        Write-Host (Msg '{0} adicionado ao PATH do usu\u00e1rio. Abra um terminal novo para usar o comando hud.' $dir)
    }
    if ($ehWindows) {
        $sessao = @(Get-EntradasPath $env:Path | Where-Object { (Get-ChavePath $_) -eq $chave }).Count -gt 0
        if (-not $sessao) { $env:Path = ((@(Get-EntradasPath $env:Path) + $dir) -join ';') }
    }
}

function Uninstall-Hud($Cfg) {
    $dir = $Cfg.Dir
    $alvo = Join-Path $dir 'hud.exe'
    $avisos = Join-Path $dir 'hud-licenses'
    if ($null -ne (Get-Atributos $dir)) { Assert-SemLink $dir }
    foreach ($p in $alvo, $avisos, (Join-Path $avisos 'LICENSES')) { Assert-SemLink $p }
    $removidos = 0
    if ($null -ne (Get-Atributos $alvo)) {
        try { Remove-Item -LiteralPath $alvo -Force } catch {
            throw (Msg 'N\u00e3o consegui apagar {0}: {1} Se o HUD estiver aberto, feche-o.' $alvo $_.Exception.Message)
        }
        $removidos++
    }
    if ($null -ne (Get-Atributos $avisos)) { Remove-Item -LiteralPath $avisos -Recurse -Force; $removidos++ }
    if (($null -ne (Get-Atributos $dir)) -and -not (Get-ChildItem -LiteralPath $dir -Force | Select-Object -First 1)) {
        Remove-Item -LiteralPath $dir -Force
    }
    $chave = Get-ChavePath $dir
    $userPath = Get-PathUsuario
    $resto = @(Get-EntradasPath $userPath | Where-Object { (Get-ChavePath $_) -ne $chave })
    if ($resto.Count -ne @(Get-EntradasPath $userPath).Count) {
        Set-PathUsuario ($resto -join ';')
        Write-Host (Msg '{0} removido do PATH do usu\u00e1rio.' $dir)
    }
    if ($removidos -gt 0) {
        Write-Host (Msg 'HUD desinstalado de {0}. A configura\u00e7\u00e3o e os dados do HUD n\u00e3o foram tocados.' $dir)
    } else {
        Write-Host (Msg 'Nada para remover em {0}.' $dir)
    }
}

$tlsAntes = [Net.ServicePointManager]::SecurityProtocol
$ok = $false
try {
    # TLS 1.2+ (o 5.1 ainda oferece TLS 1.0 em alguns Windows); o valor antigo volta no fim.
    $tls = [Net.SecurityProtocolType]::Tls12
    if ([enum]::GetNames([Net.SecurityProtocolType]) -contains 'Tls13') { $tls = $tls -bor [Net.SecurityProtocolType]::Tls13 }
    [Net.ServicePointManager]::SecurityProtocol = $tls
    $cfg = Get-Config
    if ($Remover) { Uninstall-Hud $cfg } else { Install-Hud $cfg }
    $ok = $true
} catch {
    $host.UI.WriteErrorLine('hud: ' + $_.Exception.Message)
} finally {
    [Net.ServicePointManager]::SecurityProtocol = $tlsAntes
}
if (-not $ok) {
    # No modo arquivo, sai com 1; no `irm | iex`, nao fecha o terminal de quem rodou.
    if ($ModoArquivo) { exit 1 }
    $global:LASTEXITCODE = 1
}
} $Uninstall.IsPresent ([bool]$PSCommandPath)
