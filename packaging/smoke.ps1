# Smoke do binario autocontido no Windows (equivale ao packaging/smoke.sh),
# fora do repositorio e numa pasta temporaria:
#   1. hud.exe --version
#   2. hud.exe --check -c <config so do usuario> (Claude e Codex desligados, sai com 0)
#      No Windows a config, o Vault e os dados ficam dentro do perfil simulado
#      (USERPROFILE = <tmp>\home): fora dele o HUD recusa a config. Numa sessao
#      elevada (o runner do GitHub roda como administrador) o --check sai com 1
#      so pelo aviso "rodando como administrador"; esse caso e aceito.
#   3. isolamento: modulos plantados (re, json, curses, os, sitecustomize,
#      encodings, hud...) no diretorio atual e em PYTHONPATH/PYTHONHOME/
#      PYTHONSTARTUP nao podem ser carregados. Se algum for, ele grava uma
#      marca e o teste falha.
# Uso: pwsh -NoProfile -File packaging/smoke.ps1 caminho\do\hud.exe
# Tambem roda no pwsh do Linux/macOS (contra o binario de la), para testar o
# proprio script. Arquivo so com ASCII: o Windows PowerShell 5.1 le .ps1 sem
# BOM como ANSI.
param([Parameter(Mandatory = $true)][string]$Bin)
$ErrorActionPreference = 'Stop'
Set-StrictMode -Version 3

$Bin = (Resolve-Path -LiteralPath $Bin).ProviderPath
$naoWindows = (Test-Path variable:IsWindows) -and -not $IsWindows
$dir = Join-Path ([IO.Path]::GetTempPath()) ("hud-smoke-" + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $dir | Out-Null
if ($naoWindows) {
    & chmod 700 -- $dir
} else {
    # Pasta so do usuario atual (o equivalente ao chmod 700).
    & icacls $dir /inheritance:r /grant:r "$($env:USERNAME):(OI)(CI)F" | Out-Null
    if ($LASTEXITCODE -ne 0) { throw 'icacls falhou' }
}

function Invoke-Hud {
    # Roda o binario com um ambiente minimo (o "env -i" do smoke.sh) mais $Extra.
    param([string[]]$HudArgs, [hashtable]$Extra = @{})
    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $Bin
    $psi.Arguments = ($HudArgs | ForEach-Object { '"' + $_ + '"' }) -join ' '
    $psi.WorkingDirectory = $dir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.RedirectStandardInput = $true
    $psi.EnvironmentVariables.Clear()
    $home_ = Join-Path $dir 'home'
    if ($naoWindows) {
        $psi.EnvironmentVariables['PATH'] = '/usr/bin:/bin'
        $psi.EnvironmentVariables['HOME'] = $home_
    } else {
        $sys = [Environment]::GetFolderPath('System')
        $psi.EnvironmentVariables['SystemRoot'] = $env:SystemRoot
        $psi.EnvironmentVariables['PATH'] = "$sys;$env:SystemRoot"
        $psi.EnvironmentVariables['PATHEXT'] = '.COM;.EXE;.BAT;.CMD'
        $psi.EnvironmentVariables['TEMP'] = $env:TEMP   # o onefile extrai o Python aqui
        $psi.EnvironmentVariables['TMP'] = $env:TEMP
        $psi.EnvironmentVariables['USERNAME'] = $env:USERNAME
        $psi.EnvironmentVariables['USERPROFILE'] = $home_
        $psi.EnvironmentVariables['APPDATA'] = Join-Path $home_ 'AppData\Roaming'
        $psi.EnvironmentVariables['LOCALAPPDATA'] = Join-Path $home_ 'AppData\Local'
    }
    foreach ($k in $Extra.Keys) { $psi.EnvironmentVariables[$k] = $Extra[$k] }
    $p = [Diagnostics.Process]::Start($psi)
    $p.StandardInput.Close()
    $out = $p.StandardOutput.ReadToEndAsync()
    $err = $p.StandardError.ReadToEndAsync()
    if (-not $p.WaitForExit(120000)) { $p.Kill(); throw "hud $HudArgs nao terminou em 120 s" }
    $p.WaitForExit()
    return [pscustomobject]@{ Code = $p.ExitCode; Out = $out.Result + $err.Result }
}

function Test-CheckOk($r) {
    # --check: 0, ou 1 quando o unico aviso e o de sessao elevada (Windows).
    if ($r.Code -eq 0) { return $true }
    if ($r.Code -ne 1 -or $naoWindows) { return $false }
    $avisos = @([regex]::Matches($r.Out, '(?m)^aviso: .*$') | ForEach-Object { $_.Value.Trim() })
    if ($avisos.Count -eq 0) { return $false }
    foreach ($a in $avisos) { if ($a -notmatch '^aviso: rodando como administrador') { return $false } }
    return $true
}

function Fail([string]$msg, [string]$out = '') {
    if ($out) { [Console]::Error.WriteLine($out) }
    throw "FALHOU: $msg"
}

try {
    $homeDir = Join-Path $dir 'home'
    $vault = Join-Path $homeDir 'vault'
    New-Item -ItemType Directory -Path $homeDir, $vault | Out-Null
    $mark = Join-Path $dir 'MARCA'

    Write-Output '== hud --version'
    $r = Invoke-Hud @('--version')
    if ($r.Code -ne 0 -or $r.Out.Trim() -notmatch '^hud \d+\.\d+\.\d+') { Fail 'hud --version' $r.Out }
    Write-Output $r.Out.Trim()

    # TOML com strings literais ('...'): as barras invertidas do Windows ficam como estao.
    $cfg = Join-Path $homeDir 'config.toml'
    $eco = if ($naoWindows) { '"echo", "ok"' } else { '"hostname"' }
    $toml = @"
vault = '$vault'
data_dir = '$(Join-Path $homeDir 'dados')'

[[command]]
name = "Eco"
argv = [$eco]

[claude]
enabled = false

[codex]
enabled = false
"@
    [IO.File]::WriteAllText($cfg, $toml, (New-Object Text.UTF8Encoding($false)))
    if ($naoWindows) { & chmod 600 -- $cfg }
    Write-Output '== hud --check'
    $r = Invoke-Hud @('--check', '-c', $cfg)
    if (-not (Test-CheckOk $r)) { Fail "hud --check saiu com erro ($($r.Code))" $r.Out }
    Write-Output $r.Out.TrimEnd()

    Write-Output '== isolamento (modulos plantados)'
    $mods = 're json curses os sys locale argparse pathlib tomllib subprocess threading ctypes sitecustomize usercustomize'.Split(' ')
    foreach ($mod in $mods) {
        $py = "open(r'$mark', 'a').write('$mod carregado\n')`nraise SystemExit('PLANTADO: $mod')`n"
        [IO.File]::WriteAllText((Join-Path $dir "$mod.py"), $py)
    }
    $re = Get-Content -Raw -LiteralPath (Join-Path $dir 're.py')
    foreach ($d in 'hud', 'encodings', 'Lib', 'DLLs') { New-Item -ItemType Directory -Path (Join-Path $dir $d) -Force | Out-Null }
    foreach ($f in 'hud\__init__.py', 'hud\__main__.py', 'encodings\__init__.py', 'Lib\os.py', 'Lib\re.py', 'DLLs\_curses.py') {
        [IO.File]::WriteAllText((Join-Path $dir $f), $re)
    }
    foreach ($v in '3.11', '3.12', '3.13', '3.14') {
        $lib = Join-Path $dir "lib\python$v"
        New-Item -ItemType Directory -Path $lib -Force | Out-Null
        [IO.File]::WriteAllText((Join-Path $lib 'os.py'), $re)
    }
    $startup = Join-Path $dir 'startup.py'
    [IO.File]::WriteAllText($startup, "open(r'$mark', 'a').write('PYTHONSTARTUP carregado\n')`n")

    $casos = @(
        @{ PYTHONPATH = $dir },
        @{ PYTHONHOME = $dir },
        @{ PYTHONPATH = $dir; PYTHONHOME = $dir; PYTHONSTARTUP = $startup; PYTHONINSPECT = '1'; PYTHONUSERBASE = $dir; PYTHONSAFEPATH = '0' },
        @{ PYTHONVERBOSE = '1'; PYTHONWARNINGS = 'error'; PYTHONDEBUG = '1' }
    )
    foreach ($vars in $casos) {
        $desc = ($vars.Keys | Sort-Object | ForEach-Object { "$_=$($vars[$_])" }) -join ' '
        Write-Output "   $desc"
        $r = Invoke-Hud @('--check', '-c', $cfg) $vars
        if (-not (Test-CheckOk $r)) { Fail "hud --check saiu com erro ($($r.Code)) com $desc" $r.Out }
        # PYTHONVERBOSE=1 faria o Python listar cada import ("import ..."/"# ...").
        if ($r.Out -match '(?m)^(import |# )') { Fail "variavel PYTHON* respeitada ($desc)" $r.Out }
    }
    if (Test-Path -LiteralPath $mark) { Fail 'modulo plantado foi carregado.' (Get-Content -Raw -LiteralPath $mark) }
    Write-Output '   nenhum modulo plantado foi carregado'

    # Controle: prova que a armadilha funciona num Python comum sem -I.
    $py = Get-Command python3, python -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($py) {
        $ctl = Join-Path $dir 'controle'
        New-Item -ItemType Directory -Path $ctl | Out-Null
        Copy-Item -LiteralPath (Join-Path $dir 'json.py') -Destination $ctl
        $old = $env:PYTHONPATH
        try {
            $env:PYTHONPATH = $ctl
            Push-Location ([IO.Path]::GetTempPath())
            & $py.Source -c 'import json' 2>$null | Out-Null
        } catch {
        } finally {
            Pop-Location
            $env:PYTHONPATH = $old
        }
        if (Test-Path -LiteralPath $mark) {
            Write-Output '   controle: python sem -I carregou o json plantado (armadilha valida)'
        } else {
            Fail 'o controle nao carregou o modulo plantado; o teste nao prova nada.'
        }
    }
    Write-Output 'smoke ok'
} finally {
    Remove-Item -LiteralPath $dir -Recurse -Force -ErrorAction SilentlyContinue
}
# Sucesso explicito: o "shell: pwsh" do GitHub Actions termina com exit $LASTEXITCODE,
# e o ultimo comando nativo (o python do controle, que cai na armadilha) sai com 1.
exit 0
