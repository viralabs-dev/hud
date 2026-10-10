# Condutor do roteiro de validacao da tela no Windows (docs/roteiro-windows.md).
#
# Prepara uma pasta de teste com dados sinteticos (config sem agentes reais,
# Vault de exemplo com um Kanban, agenda), coleta o ambiente (hud --version,
# Windows, PowerShell, terminal), mostra cada passo do roteiro, abre o hud.exe
# numa janela nova do terminal em teste quando o passo pede, pergunta o
# resultado (ok/falhou/pulado + observacao) e grava um relatorio
# roteiro-windows-<data>-<terminal>.md na pasta de teste.
#
# Uso (PowerShell comum, sem "Executar como administrador"):
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\roteiro-windows.ps1 -Terminal wt
#   powershell -NoProfile -ExecutionPolicy Bypass -File .\scripts\roteiro-windows.ps1 -Terminal conhost
# Opcoes:
#   -Hud C:\caminho\hud.exe   binario a testar (padrao: o hud do PATH)
#   -Pasta C:\caminho         pasta de teste (padrao: %LOCALAPPDATA%\hud-roteiro)
#   -Roteiro arquivo.md       o roteiro (padrao: ..\docs\roteiro-windows.md)
#   -SoPreparar               so cria a pasta de teste e mostra como abrir o HUD
#   -Ensaio                   nao pergunta nada nem abre o HUD (teste do proprio
#                             script; e o modo usado fora do Windows, no pwsh do
#                             Linux ou do macOS)
#
# Nao instala nada, nao muda configuracao do sistema e roda como usuario comum.
# O HUD de teste roda com USERPROFILE, APPDATA, LOCALAPPDATA e TEMP apontando
# para a pasta de teste (so no processo dele): a config, os dados, o Vault, as
# sessoes de agentes e as customizacoes reais nao sao lidos nem tocados.
#
# Arquivo so com ASCII: o Windows PowerShell 5.1 le .ps1 sem BOM como ANSI. Os
# acentos dos dados de teste vem de \uXXXX (funcao U); os do roteiro vem do
# proprio .md, lido em UTF-8.
param(
    [string]$Terminal = '',
    [string]$Hud = '',
    [string]$Pasta = '',
    [string]$Roteiro = '',
    [switch]$SoPreparar,
    [switch]$Ensaio
)
Set-StrictMode -Version 2
$ErrorActionPreference = 'Stop'

function U([string]$Texto) {
    # \uXXXX vira o caractere (pares substitutos formam o emoji).
    return [regex]::Unescape($Texto)
}

$ehWindows = ($PSVersionTable.PSEdition -ne 'Core') -or ((Test-Path variable:IsWindows) -and $IsWindows)
if (-not $ehWindows -and -not $Ensaio) {
    Write-Output 'fora do Windows: modo de ensaio (nada e perguntado e o HUD nao e aberto).'
    $Ensaio = [switch]$true
}

# A linha que o passo da nota cola na ENTRADA (o roteiro cita o mesmo texto).
$LinhaTeste = U 'Teste a\u00e7\u00e3o ma\u00e7\u00e3 cora\u00e7\u00e3o \u2500\u2502\u256d \u2588\u2591 \u25cf\u25b6\u25a0 \uD83D\uDE80'
# Configs que o roteiro abre: "abre o HUD (base)" e "abre o HUD (abas)".
$Modos = @('base', 'abas')

function Join([string]$Base) {
    # Join-Path de varios pedacos (o do 5.1 so junta dois).
    $p = $Base
    foreach ($parte in $args) { $p = Join-Path $p $parte }
    return $p
}

function Write-Utf8([string]$Caminho, [string]$Texto, [bool]$Bom = $false) {
    [IO.File]::WriteAllText($Caminho, $Texto, (New-Object Text.UTF8Encoding($Bom)))
}

function TomlStr([string]$s) {
    # String basica do TOML: barras invertidas e aspas escapadas.
    return '"' + $s.Replace('\', '\\').Replace('"', '\"') + '"'
}

function PsStr([string]$s) {
    return "'" + $s.Replace("'", "''") + "'"
}

function Test-Link([string]$Caminho) {
    $fi = New-Object IO.DirectoryInfo $Caminho
    $fi.Refresh()
    if ([int]$fi.Attributes -eq -1) { return $false }
    return [bool]($fi.Attributes -band [IO.FileAttributes]::ReparsePoint)
}

function New-Dir([string]$Caminho) {
    if (-not (Test-Path -LiteralPath $Caminho)) { New-Item -ItemType Directory -Path $Caminho | Out-Null }
}

# ---------------------------------------------------------------- caminhos

$raiz = Split-Path -Parent $PSScriptRoot
if (-not $Roteiro) { $Roteiro = Join $raiz 'docs' 'roteiro-windows.md' }
if (-not (Test-Path -LiteralPath $Roteiro)) { throw "roteiro nao encontrado: $Roteiro (use -Roteiro)" }
$Roteiro = (Resolve-Path -LiteralPath $Roteiro).ProviderPath

$Terminal = $Terminal.ToLower()
if (-not $Terminal) {
    if ($Ensaio -or $SoPreparar) { $Terminal = 'wt' }
    else {
        $Terminal = (Read-Host 'Terminal em teste: wt (Windows Terminal + PowerShell 7) ou conhost (console classico + PowerShell 5.1)').Trim().ToLower()
    }
}
if ($Terminal -ne 'wt' -and $Terminal -ne 'conhost') { throw "terminal desconhecido: '$Terminal' (use wt ou conhost)" }

if (-not $Pasta) {
    if ($ehWindows) { $Pasta = Join $env:LOCALAPPDATA 'hud-roteiro' }
    else { $Pasta = Join ([IO.Path]::GetTempPath()) 'hud-roteiro' }
}
$marca = Join $Pasta '.hud-roteiro'
if ((Test-Path -LiteralPath $Pasta) -and -not (Test-Path -LiteralPath $marca)) {
    if (@(Get-ChildItem -LiteralPath $Pasta -Force).Count -gt 0) {
        throw "$Pasta existe, nao esta vazia e nao foi criada por este script: escolha outra com -Pasta"
    }
}
New-Dir $Pasta
if (Test-Link $Pasta) { throw "$Pasta e um link ou juncao: escolha outra pasta" }
Write-Utf8 $marca "pasta de teste do scripts/roteiro-windows.ps1 do HUD`n"
$Pasta = (Resolve-Path -LiteralPath $Pasta).ProviderPath

$casa = Join $Pasta 'home'
$roaming = Join $casa 'AppData' 'Roaming'
$local = Join $casa 'AppData' 'Local'
$temp = Join $local 'Temp'
$cfgDir = Join $roaming 'hud'
$dados = Join $local 'hud'
$vault = Join $casa 'Vault'
$outra = Join $casa 'Outra pasta'
$cfg = @{ base = (Join $cfgDir 'config.toml'); abas = (Join $cfgDir 'config-abas.toml') }

if (-not $Hud) {
    $cmd = Get-Command hud -CommandType Application -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($cmd) { $Hud = $cmd.Source }
}
if ($Hud) {
    if (-not (Test-Path -LiteralPath $Hud)) { throw "hud nao encontrado: $Hud" }
    $Hud = (Resolve-Path -LiteralPath $Hud).ProviderPath
} elseif (-not $Ensaio) {
    throw 'hud.exe nao esta no PATH: instale (README, Instalar no Windows) ou use -Hud C:\caminho\hud.exe'
}

# ---------------------------------------------------------------- dados sinteticos

function New-Sandbox {
    # Recria home\ do zero a cada execucao; os relatorios ficam fora dela.
    if (Test-Path -LiteralPath $casa) {
        if (Test-Link $casa) { throw "$casa e um link ou juncao: nao apago" }
        Remove-Item -LiteralPath $casa -Recurse -Force
    }
    foreach ($d in @($casa, $roaming, $local, $temp, $cfgDir, $dados, $vault, (Join $vault '.obsidian'),
            (Join $vault 'Notas'), (Join $vault 'Projetos' 'Roteiro' '05-backlog'), $outra)) {
        New-Dir $d
    }
    if (-not $ehWindows) { & chmod -R go-rwx -- $Pasta }

    $hoje = Get-Date
    $dia = { param($n) $hoje.AddDays($n).ToString('yyyy-MM-dd') }

    # Comandos do painel: so programas do Windows que leem, sem shell.
    $comandos = @'
[[command]]
name = "Nome da m\u00e1quina"
argv = ["hostname"]

[[command]]
name = "Usu\u00e1rio"
argv = ["whoami"]

[[command]]
name = "Fuso hor\u00e1rio"
argv = ["tzutil", "/g"]

[[command]]
name = "Fusos (lista longa)"
argv = ["tzutil", "/l"]
max_lines = 1000

[[command]]
name = "Drivers (lista longa)"
argv = ["driverquery", "/FO", "TABLE"]
max_lines = 1000

[[command]]
name = "Processos do HUD"
argv = ["tasklist", "/FI", "IMAGENAME eq hud.exe"]

[[command]]
name = "Ping local"
argv = ["ping", "-n", "4", "127.0.0.1"]

[[command]]
name = "Tempo-limite"
argv = ["ping", "-n", "30", "127.0.0.1"]
timeout = 5

[[command]]
name = "Confirma\u00e7\u00e3o"
argv = ["whoami"]
confirm = true

[[command]]
name = "Falha de prop\u00f3sito"
argv = ["whoami", "/opcao-invalida"]
'@
    $topo = "# Config de teste do roteiro do Windows (scripts/roteiro-windows.ps1). Sem dados reais.`n" +
        "vault = $(TomlStr $vault)`n" +
        "data_dir = $(TomlStr $dados)`n" +
        "refresh_seconds = 1`n" +
        "vault_scan_seconds = 5`n`n"
    $semAgentes = "`n[claude]`nenabled = false`n`n[codex]`nenabled = false`n`n[opencode]`nenabled = false`n"
    # Agentes falsos: whoami recusa os argumentos e a "resposta" e o erro dele.
    $falsos = "`n[claude]`nenabled = true`nexecutable = `"whoami`"`nprofile = `"leitura`"`ntimeout = 30`n" +
        "`n[codex]`nenabled = true`nexecutable = `"whoami`"`nprofile = `"leitura`"`ntimeout = 30`n" +
        "`n[opencode]`nenabled = true`nexecutable = `"whoami`"`nprofile = `"leitura`"`ntimeout = 30`n"
    Write-Utf8 $cfg.base ($topo + $comandos + "`n" + $semAgentes)
    Write-Utf8 $cfg.abas ($topo + $comandos + "`n" + $falsos)

    $agenda = (U ("- [ ] {0} 10:00 Item atrasado de teste`n" +
        "- [ ] {1} 09:00 Caf\u00e9 da manh\u00e3: a\u00e7\u00e3o, p\u00e3o`n" +
        "- [ ] {2} 14:00 Reuni\u00e3o \u2500\u2502\u256d caixas`n" +
        "- [ ] {3} Revisar emoji \uD83D\uDE80 sem hora`n")) -f (& $dia (-1)), (& $dia 0), (& $dia 1), (& $dia 3)
    Write-Utf8 (Join $dados 'agenda.md') $agenda

    Write-Utf8 (Join $vault 'Bem-vindo.md') ((U ("# Bem-vindo ao Vault de teste`n`n" +
        "Nota sint\u00e9tica do roteiro do HUD no Windows. Palavras para a busca: acentua\u00e7\u00e3o, " +
        "cora\u00e7\u00e3o, p\u00e3o de queijo.`n`n" +
        "- [ ] Tarefa do Vault \uD83D\uDCC5 {0} \u23F0 16:00`n")) -f (& $dia 0))
    for ($i = 1; $i -le 5; $i++) {
        Write-Utf8 (Join $vault 'Notas' ("Nota 0$i.md")) (U ("# Nota $i`n`nTexto de exemplo $i com acentua\u00e7\u00e3o " +
            "e s\u00edmbolos \u2500\u2502\u256d \u2588\u2591 \u25cf\u25b6\u25a0.`n"))
    }
    Write-Utf8 (Join $vault 'Projetos' 'Roteiro' 'Roteiro.md') (U ("# Roteiro`n`nProjeto de exemplo do roteiro de " +
        "valida\u00e7\u00e3o.`n"))
    $kanban = U ("---`nkanban-plugin: board`n---`n`n" +
        "## A fazer`n`n- [ ] Conferir acentua\u00e7\u00e3o: a\u00e7\u00e3o, cora\u00e7\u00e3o, p\u00e3o`n" +
        "- [ ] Desenhar caixas \u2500\u2502\u256d\u256e\u2570\u256f`n`n" +
        "## Em andamento`n`n- [ ] Barras \u2588\u2591 e marcas \u25cf\u25b6\u25a0`n" +
        "- [ ] Lan\u00e7ar foguete \uD83D\uDE80`n`n" +
        "## Bloqueado`n`n- [ ] Aguardando Windows de teste`n`n" +
        "## Conclu\u00eddo`n`n- [x] Preparar dados sint\u00e9ticos`n`n" +
        "%% kanban:settings`n" + '```' + "`n{`"kanban-plugin`":`"board`"}`n" + '```' + "`n%%`n")
    Write-Utf8 (Join $vault 'Projetos' 'Roteiro' '05-backlog' 'Kanban (Roteiro).md') $kanban
    Write-Utf8 (Join $outra 'leia-me.md') (U "# Outra pasta`n`nPasta de exemplo para o /pasta.`n")
    if (-not $ehWindows) { & chmod -R go-rwx -- $Pasta }

    # Lancadores: abrem o HUD de teste com o perfil simulado (UTF-8 com BOM: o
    # caminho pode ter acento, e o 5.1 le .ps1 sem BOM como ANSI).
    foreach ($m in $Modos) {
        $hudLit = if ($Hud) { PsStr $Hud } else { "'hud.exe'" }
        $l = "# Gerado por scripts/roteiro-windows.ps1: abre o HUD de teste (config $m).`r`n" +
            "`$env:USERPROFILE = $(PsStr $casa)`r`n" +
            "`$env:APPDATA = $(PsStr $roaming)`r`n" +
            "`$env:LOCALAPPDATA = $(PsStr $local)`r`n" +
            "`$env:TEMP = $(PsStr $temp)`r`n" +
            "`$env:TMP = $(PsStr $temp)`r`n" +
            "Set-Location -LiteralPath $(PsStr $casa)`r`n" +
            "try { `$Host.UI.RawUI.WindowTitle = 'HUD de teste ($m)' } catch { }`r`n" +
            "Write-Host 'HUD de teste (config $m). Feche com /sair; esta janela fica aberta para conferir a saida.'`r`n" +
            "& $hudLit -c $(PsStr $cfg[$m])`r`n" +
            "Write-Host ('hud saiu com codigo ' + `$LASTEXITCODE)`r`n"
        Write-Utf8 (Join $Pasta "abrir-hud-$m.ps1") $l $true
    }
}

function Open-Hud([string]$Modo) {
    # Abre o HUD de teste numa janela nova do terminal em teste.
    $lanc = Join $Pasta "abrir-hud-$Modo.ps1"
    $resto = "-NoLogo -NoProfile -NoExit -ExecutionPolicy Bypass -File `"$lanc`""
    if ($Terminal -eq 'wt') {
        $wt = Get-Command wt.exe -ErrorAction SilentlyContinue | Select-Object -First 1
        if (-not $wt) { Write-Host 'wt.exe nao encontrado: o Windows Terminal esta instalado?'; return }
        $sh = Get-Command pwsh.exe -ErrorAction SilentlyContinue | Select-Object -First 1
        $shell = if ($sh) { $sh.Source } else { 'powershell.exe' }
        if (-not $sh) { Write-Host 'pwsh.exe (PowerShell 7) nao encontrado: abrindo com o Windows PowerShell 5.1.' }
        Start-Process -FilePath $wt.Source -ArgumentList "-w new `"$shell`" $resto"
    } else {
        $conhost = Join $env:SystemRoot 'System32' 'conhost.exe'
        $ps51 = Join $env:SystemRoot 'System32' 'WindowsPowerShell' 'v1.0' 'powershell.exe'
        Start-Process -FilePath $conhost -ArgumentList "`"$ps51`" $resto"
    }
}

# ---------------------------------------------------------------- ambiente

function Invoke-Texto([string]$Exe, [string[]]$Argumentos) {
    # Saida (stdout + stderr) e codigo de um programa; o perfil simulado vale
    # tambem aqui, para o --check ler a config de teste como o HUD vai ler.
    $ErrorActionPreference = 'Continue'  # no 5.1, stderr de programa com 2>&1 e Stop vira excecao
    $salvo = @{}
    $vars = @{ USERPROFILE = $casa; APPDATA = $roaming; LOCALAPPDATA = $local; TEMP = $temp; TMP = $temp }
    if (-not $ehWindows) { $vars = @{ HOME = $casa } }
    foreach ($k in $vars.Keys) { $salvo[$k] = [Environment]::GetEnvironmentVariable($k); [Environment]::SetEnvironmentVariable($k, $vars[$k]) }
    try {
        $out = & $Exe @Argumentos 2>&1 | Out-String
        return [pscustomobject]@{ Code = $LASTEXITCODE; Out = $out.TrimEnd() }
    } catch {
        return [pscustomobject]@{ Code = -1; Out = $_.Exception.Message }
    } finally {
        foreach ($k in $salvo.Keys) { [Environment]::SetEnvironmentVariable($k, $salvo[$k]) }
    }
}

function Get-Ambiente {
    $a = [ordered]@{}
    $a['Data'] = (Get-Date).ToString('yyyy-MM-dd HH:mm')
    $a['Terminal em teste'] = if ($Terminal -eq 'wt') { 'Windows Terminal + PowerShell 7' } else { 'console classico (conhost) + Windows PowerShell 5.1' }
    $a['hud'] = if ($Hud) { $Hud } else { '(nao informado)' }
    $a['hud --version'] = if ($Hud) { (Invoke-Texto $Hud @('--version')).Out } else { '(ensaio sem -Hud)' }
    $a['Sistema'] = [Environment]::OSVersion.VersionString
    if ($ehWindows) {
        try {
            $r = Get-ItemProperty 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
            $build = [int]$r.CurrentBuild
            $nome = if ($build -ge 22000) { 'Windows 11' } else { 'Windows 10' }
            $versao = if ($r.PSObject.Properties['DisplayVersion']) { $r.DisplayVersion } elseif ($r.PSObject.Properties['ReleaseId']) { $r.ReleaseId } else { '?' }
            $ubr = if ($r.PSObject.Properties['UBR']) { $r.UBR } else { '?' }
            $a['Windows'] = "$nome $versao (compilacao $($r.CurrentBuild).$ubr, $($r.EditionID))"
        } catch { $a['Windows'] = "nao consegui ler o registro: $($_.Exception.Message)" }
    }
    $a['PowerShell do condutor'] = "$($PSVersionTable.PSVersion) ($($PSVersionTable.PSEdition))"
    if ($ehWindows) {
        if ($Terminal -eq 'wt') {
            $v = ''
            try { $v = (Get-AppxPackage -Name 'Microsoft.WindowsTerminal*' | Select-Object -First 1).Version } catch { }
            $a['Windows Terminal'] = if ($v) { "$v" } else { '(nao detectado: veja Configuracoes > Sobre)' }
            $sh = Get-Command pwsh.exe -ErrorAction SilentlyContinue | Select-Object -First 1
            $a['PowerShell da janela do HUD'] = if ($sh) { 'pwsh ' + (Invoke-Texto $sh.Source @('-NoProfile', '-Command', '$PSVersionTable.PSVersion.ToString()')).Out } else { '(sem pwsh: Windows PowerShell 5.1)' }
        } else {
            $c = Join $env:SystemRoot 'System32' 'conhost.exe'
            $a['conhost'] = (Get-Item -LiteralPath $c).VersionInfo.ProductVersion
            $ps51 = Join $env:SystemRoot 'System32' 'WindowsPowerShell' 'v1.0' 'powershell.exe'
            $a['PowerShell da janela do HUD'] = 'powershell ' + (Invoke-Texto $ps51 @('-NoProfile', '-Command', '$PSVersionTable.PSVersion.ToString()')).Out
        }
        $a['Dentro do condutor'] = if ($env:WT_SESSION) { 'Windows Terminal' } else { 'outro (conhost ou outro terminal)' }
    }
    try { $a['Pagina de codigo do console'] = "$([Console]::OutputEncoding.CodePage)" } catch { }
    return $a
}

# ---------------------------------------------------------------- roteiro

function Read-Roteiro([string]$Caminho) {
    $texto = [IO.File]::ReadAllText($Caminho, [Text.Encoding]::UTF8)
    $passos = New-Object Collections.ArrayList
    $atual = $null
    $campo = $null
    foreach ($linha in ($texto -split "`r?`n")) {
        if ($linha -match '^### (P\d+) \S+ (.+)$') {
            $atual = [ordered]@{ Id = $Matches[1]; Titulo = $Matches[2]; Sessao = ''; Faca = ''; Confira = '' }
            [void]$passos.Add($atual)
            $campo = $null
            continue
        }
        if ($null -eq $atual) { continue }
        if ($linha -match '^##') { $atual = $null; continue }
        if ($linha -match '^- \*\*([^*:]+):\*\*\s?(.*)$') {
            $nome = $Matches[1]; $valor = $Matches[2]
            $campo = $null
            if ($nome.StartsWith('Sess')) { $campo = 'Sessao' }
            elseif ($nome.StartsWith('Fa')) { $campo = 'Faca' }
            elseif ($nome.StartsWith('Conf')) { $campo = 'Confira' }
            if ($campo) { $atual[$campo] = $valor }
            continue
        }
        if ($campo -and $linha -match '^\s+\S') { $atual[$campo] = $atual[$campo] + ' ' + $linha.Trim() }
    }
    return $passos
}

function Get-Modo($Passo) {
    if ($Passo.Sessao -match 'abre o HUD \((\w+)\)') { return $Matches[1] }
    return ''
}

function Get-Mei {
    if (-not (Test-Path -LiteralPath $temp)) { return @() }
    return @(Get-ChildItem -LiteralPath $temp -Directory -Filter '_MEI*' -ErrorAction SilentlyContinue | ForEach-Object { $_.Name })
}

function Get-Sobras {
    # Processos hud.exe e pastas _MEI deixados pelo HUD de teste.
    $procs = @()
    if ($ehWindows) { $procs = @(Get-Process -Name hud -ErrorAction SilentlyContinue | ForEach-Object { "hud.exe pid $($_.Id)" }) }
    $mei = @(Get-Mei | ForEach-Object { "$temp\$_" })
    $tudo = @($procs + $mei)
    if ($tudo.Count -eq 0) { return 'nenhum hud.exe rodando e nenhuma pasta _MEI na TEMP de teste' }
    return 'sobrou: ' + ($tudo -join '; ')
}

function Esc([string]$s) {
    return ($s -replace '\|', '\|' -replace "`r?`n", ' ')
}

function Save-Relatorio([string]$Caminho, $Ambiente, $Resultados, [int]$Total) {
    $sb = New-Object Text.StringBuilder
    [void]$sb.AppendLine((U "# Roteiro da tela no Windows \u00b7 $($Ambiente['Data']) \u00b7 $Terminal"))
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine((U 'Gerado por `scripts/roteiro-windows.ps1` a partir de `docs/roteiro-windows.md`. Dados sint\u00e9ticos, perfil simulado.'))
    if ($Ensaio) { [void]$sb.AppendLine(''); [void]$sb.AppendLine((U '**Ensaio:** nada foi executado na tela; o relat\u00f3rio s\u00f3 prova o script.')) }
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine('## Ambiente')
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine('| Item | Valor |')
    [void]$sb.AppendLine('|---|---|')
    foreach ($k in $Ambiente.Keys) { [void]$sb.AppendLine("| $k | $(Esc ([string]$Ambiente[$k])) |") }
    [void]$sb.AppendLine('')
    $cont = @{}
    foreach ($r in $Resultados) { $cont[$r.Resultado] = 1 + $(if ($cont.ContainsKey($r.Resultado)) { $cont[$r.Resultado] } else { 0 }) }
    $resumo = ($cont.Keys | Sort-Object | ForEach-Object { "$($cont[$_]) $_" }) -join ', '
    [void]$sb.AppendLine('## Resultados')
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine("$($Resultados.Count) de $Total passos registrados: $resumo.")
    [void]$sb.AppendLine('')
    [void]$sb.AppendLine((U '| Passo | T\u00edtulo | Resultado | Observa\u00e7\u00e3o |'))
    [void]$sb.AppendLine('|---|---|---|---|')
    foreach ($r in $Resultados) {
        [void]$sb.AppendLine("| $($r.Id) | $(Esc $r.Titulo) | $($r.Resultado) | $(Esc $r.Obs) |")
    }
    Write-Utf8 $Caminho $sb.ToString()
}

# ---------------------------------------------------------------- execucao

$saidaAntiga = $null
try { $saidaAntiga = [Console]::OutputEncoding; [Console]::OutputEncoding = New-Object Text.UTF8Encoding($false) } catch { $saidaAntiga = $null }
try {
    $passos = Read-Roteiro $Roteiro
    if ($passos.Count -eq 0) { throw "nenhum passo (### Pnn) em $Roteiro" }
    foreach ($p in $passos) {
        $m = Get-Modo $p
        if ($m -and ($Modos -notcontains $m)) { throw "$($p.Id): config desconhecida '$m' (conhecidas: $($Modos -join ', '))" }
    }

    New-Sandbox
    Write-Output "pasta de teste: $Pasta"
    foreach ($m in $Modos) {
        Write-Output "  config $m : $($cfg[$m])"
        Write-Output "  abrir o HUD ($m) nesta janela: powershell -NoProfile -ExecutionPolicy Bypass -File `"$(Join $Pasta "abrir-hud-$m.ps1")`""
    }
    if ($SoPreparar) { return }

    $ambiente = Get-Ambiente
    if (-not $Ensaio) {
        $ambiente['Fonte do terminal'] = Read-Host 'Fonte do terminal (ex.: Cascadia Mono 12, Consolas 16)'
        $ambiente['winver confere'] = Read-Host 'O winver mostra a mesma versao e compilacao acima? (s/n + observacao)'
    }
    $ambiente['Passos no roteiro'] = "$($passos.Count) ($Roteiro)"
    $meiAntes = @(Get-Mei)

    Write-Output ''
    Write-Output '== ambiente'
    foreach ($k in $ambiente.Keys) { Write-Output ("  {0}: {1}" -f $k, $ambiente[$k]) }
    if ($Hud -and $ehWindows) {
        Write-Output ''
        Write-Output "== hud --check -c $($cfg.base)"
        $chk = Invoke-Texto $Hud @('--check', '-c', $cfg.base)
        Write-Output $chk.Out
        Write-Output "(saiu com $($chk.Code))"
        $ambiente['hud --check (base)'] = "saiu com $($chk.Code)"
    }

    $data = Get-Date -Format 'yyyy-MM-dd-HHmm'
    $relatorio = Join $Pasta "roteiro-windows-$data-$Terminal.md"
    $resultados = New-Object Collections.ArrayList
    $ultimoModo = 'base'
    $parar = $false
    foreach ($p in $passos) {
        if ($parar) { break }
        Write-Output ''
        Write-Output ("== {0} {1} {2}" -f $p.Id, (U '\u00b7'), $p.Titulo)
        Write-Output ((U '  Sess\u00e3o:  ') + $p.Sessao)
        Write-Output ((U '  Fa\u00e7a:    ') + $p.Faca)
        Write-Output ('  Confira: ' + $p.Confira)
        if ($p.Faca -match 'linha de teste') { Write-Output ''; Write-Output "  linha de teste: $LinhaTeste" }
        if ($p.Confira -match '_MEI') {
            $sobras = Get-Sobras
            $novas = @(Get-Mei | Where-Object { $meiAntes -notcontains $_ })
            Write-Output "  condutor: $sobras (novas desde o inicio: $($novas.Count))"
        }
        if ($Ensaio) {
            [void]$resultados.Add([pscustomobject]@{ Id = $p.Id; Titulo = $p.Titulo; Resultado = 'ensaio'; Obs = $p.Sessao })
            continue
        }
        $modo = Get-Modo $p
        if ($modo) {
            $ultimoModo = $modo
            [void](Read-Host "  Enter abre o HUD (config $modo) numa janela nova do terminal em teste")
            Open-Hud $modo
        }
        if ($p.Faca -match 'linha de teste') {
            $c = Read-Host '  Copiar a linha de teste para a area de transferencia? (s/N)'
            if ($c -match '^[sS]') { try { Set-Clipboard -Value $LinhaTeste } catch { Write-Output "  nao consegui copiar: $($_.Exception.Message)" } }
        }
        $res = ''
        while (-not $res) {
            $r = (Read-Host '  Resultado: [o]k, [f]alhou, [p]ulado, [a]brir o HUD de novo, [s]air e gravar').Trim().ToLower()
            switch ($r) {
                'o' { $res = 'ok' }
                'f' { $res = 'falhou' }
                'p' { $res = 'pulado' }
                'a' { Open-Hud $ultimoModo }
                's' { $parar = $true; $res = 'parado' }
                default { Write-Output '  responda o, f, p, a ou s' }
            }
        }
        if ($res -eq 'parado') { break }
        $obs = Read-Host '  Observacao (Enter para nenhuma)'
        if ($p.Confira -match '_MEI') { $obs = ("$obs [$sobras]").Trim() }
        [void]$resultados.Add([pscustomobject]@{ Id = $p.Id; Titulo = $p.Titulo; Resultado = $res; Obs = $obs })
        Save-Relatorio $relatorio $ambiente $resultados $passos.Count
    }
    Save-Relatorio $relatorio $ambiente $resultados $passos.Count
    Write-Output ''
    Write-Output "relatorio: $relatorio"
    Write-Output 'anexe o relatorio e as capturas a AT-055 (veja "Depois de executar" no roteiro).'
} finally {
    if ($null -ne $saidaAntiga) { try { [Console]::OutputEncoding = $saidaAntiga } catch { } }
}
