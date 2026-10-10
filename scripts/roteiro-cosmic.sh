#!/usr/bin/env bash
# Condutor do roteiro de validação da tela no COSMIC Terminal (docs/roteiro-cosmic.md).
#
# Prepara uma pasta de teste com dados sintéticos (config sem agentes reais,
# Vault de exemplo com um Kanban, agenda), coleta o ambiente (hud --version,
# cosmic-term --version, TERM, tamanho), mostra cada passo do roteiro, abre uma
# janela nova do cosmic-term na pasta de teste quando o passo pede, pergunta o
# resultado (ok/falhou/pulado + observação) e grava roteiro-cosmic-<data>.md na
# pasta de teste.
#
# Uso:
#   scripts/roteiro-cosmic.sh [--hud CAMINHO] [--pasta DIR] [--so-preparar] [--ensaio]
# Opções:
#   --hud CAMINHO   o hud a testar (padrão: o hud do PATH, senão bin/hud do repositório)
#   --pasta DIR     pasta de teste (padrão: $XDG_STATE_HOME/hud-roteiro-cosmic)
#   --roteiro ARQ   o roteiro (padrão: docs/roteiro-cosmic.md do repositório)
#   --so-preparar   só cria a pasta de teste e mostra como abrir o HUD
#   --ensaio        não pergunta nada nem abre janela (teste do próprio script;
#                   não exige cosmic-term)
#
# Só bash e coreutils (wl-copy, se existir, copia o comando para colar). Não
# instala nada e não muda a configuração do cosmic-term nem do sistema. O HUD de
# teste roda com HOME, XDG_*, CLAUDE_CONFIG_DIR, CODEX_HOME e TMPDIR apontando
# para a pasta de teste (só no processo dele): a config, os dados, o Vault, as
# sessões de agentes e as customizações reais não são lidos nem tocados.
set -euo pipefail
umask 077

ENSAIO=0
SO_PREPARAR=0
HUD=""
PASTA=""
ROTEIRO=""

erro() { printf 'roteiro-cosmic: %s\n' "$*" >&2; exit 2; }

while [[ $# -gt 0 ]]; do
    case "$1" in
        --ensaio) ENSAIO=1 ;;
        --so-preparar) SO_PREPARAR=1 ;;
        --hud) [[ $# -ge 2 ]] || erro "--hud precisa de um caminho"; HUD="$2"; shift ;;
        --pasta) [[ $# -ge 2 ]] || erro "--pasta precisa de um caminho"; PASTA="$2"; shift ;;
        --roteiro) [[ $# -ge 2 ]] || erro "--roteiro precisa de um arquivo"; ROTEIRO="$2"; shift ;;
        -h|--help) sed -n '2,/^set -euo/{/^set -euo/d;s/^# \{0,1\}//;p}' "$0"; exit 0 ;;
        *) erro "opção desconhecida: $1 (veja --help)" ;;
    esac
    shift
done

# A linha que o passo da nota cola na ENTRADA (o roteiro cita o mesmo texto).
LINHA_TESTE='Teste ação maçã coração ─│╭ █░ ●▶■ 🚀'
# Configs que o roteiro abre: "abre o HUD (base)" e "abre o HUD (abas)".
MODOS=(base abas)

# ---------------------------------------------------------------- caminhos

RAIZ="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)"
[[ -n $ROTEIRO ]] || ROTEIRO="$RAIZ/docs/roteiro-cosmic.md"
[[ -f $ROTEIRO ]] || erro "roteiro não encontrado: $ROTEIRO (use --roteiro)"
ROTEIRO="$(readlink -f -- "$ROTEIRO")"

REAL_HOME="${HOME:?HOME vazio}"
[[ -n $PASTA ]] || PASTA="${XDG_STATE_HOME:-$REAL_HOME/.local/state}/hud-roteiro-cosmic"
PASTA="$(readlink -m -- "$PASTA")"

# A pasta de teste nunca pode ser (nem ficar dentro de) a casa, a config, os
# dados ou o Vault reais.
for proibida in "$REAL_HOME" "$REAL_HOME/.config/hud" "$REAL_HOME/.local/share/hud" "$REAL_HOME/Vault" /; do
    proibida="$(readlink -m -- "$proibida")"
    if [[ $PASTA == "$proibida" ]]; then erro "$PASTA não pode ser a pasta de teste: escolha outra com --pasta"; fi
    if [[ $proibida != "$(readlink -m -- "$REAL_HOME")" && $proibida != / && $PASTA == "$proibida"/* ]]; then
        erro "$PASTA fica dentro de $proibida: escolha outra com --pasta"
    fi
done

MARCA="$PASTA/.hud-roteiro"
if [[ -L $PASTA ]]; then erro "$PASTA é link simbólico: escolha outra pasta"; fi
if [[ -d $PASTA && ! -f $MARCA ]] && [[ -n "$(ls -A -- "$PASTA")" ]]; then
    erro "$PASTA existe, não está vazia e não foi criada por este script: escolha outra com --pasta"
fi
mkdir -p -- "$PASTA"
chmod 700 -- "$PASTA"
printf 'pasta de teste do scripts/roteiro-cosmic.sh do HUD\n' > "$MARCA"

CASA="$PASTA/home"
CFG_DIR="$CASA/.config/hud"
DADOS="$CASA/.local/share/hud"
# Fora de data_dir, como no repositório (no HUD instalado, data_dir/custom é a pasta de customizações).
CUSTOM="$CASA/custom"
VAULT="$CASA/Vault"
OUTRA="$CASA/Outra pasta"
TEXTO_LONGO="$CASA/texto-longo.txt"
declare -A CFG=([base]="$CFG_DIR/config.toml" [abas]="$CFG_DIR/config-abas.toml")

if [[ -z $HUD ]]; then
    HUD="$(command -v hud || true)"
    [[ -n $HUD ]] || HUD="$RAIZ/bin/hud"
fi
[[ -f $HUD && -x $HUD ]] || erro "hud não encontrado ou não executável: $HUD (use --hud)"
HUD="$(readlink -f -- "$HUD")"

# ---------------------------------------------------------------- utilitários

q() {  # aspas simples de shell
    local s=${1//\'/\'\\\'\'}
    printf "'%s'" "$s"
}

toml() {  # string básica do TOML
    local s=${1//\\/\\\\}
    s=${s//\"/\\\"}
    printf '"%s"' "$s"
}

dia() { date -d "$1 day" +%F; }

hud_teste() {  # roda o hud com o perfil simulado (o mesmo dos lançadores)
    env HOME="$CASA" XDG_CONFIG_HOME="$CASA/.config" XDG_DATA_HOME="$CASA/.local/share" \
        XDG_STATE_HOME="$CASA/.local/state" XDG_CACHE_HOME="$CASA/.cache" \
        CLAUDE_CONFIG_DIR="$CASA/.claude" CODEX_HOME="$CASA/.codex" TMPDIR="$CASA/tmp" \
        "$HUD" "$@"
}

impressao() {  # só leitura: a lista (com datas) das pastas reais do HUD
    local d
    for d in "$REAL_HOME/.config/hud" "$REAL_HOME/.local/share/hud"; do
        if [[ -e $d ]]; then ls -laR --time-style=full-iso -- "$d" 2>/dev/null || true
        else printf 'ausente %s\n' "$d"; fi
    done | cksum
}

hud_rodando() {  # pids cujo comando cita a config de teste (/proc, sem pgrep)
    local f linha pids=()
    for f in /proc/[0-9]*/cmdline; do
        [[ -r $f ]] || continue
        linha="$(tr '\0' ' ' < "$f" 2>/dev/null || true)"
        if [[ $linha == *"$CFG_DIR/config"* && $linha != *roteiro-cosmic.sh* ]]; then
            f=${f#/proc/}; pids+=("${f%/cmdline}")
        fi
    done
    printf '%s' "${pids[*]-}"
}

# ---------------------------------------------------------------- dados sintéticos

nova_casa() {
    # Recria home/ do zero a cada execução; os relatórios ficam fora dela.
    if [[ -L $CASA ]]; then erro "$CASA é link simbólico: não apago"; fi
    rm -rf -- "$CASA"
    mkdir -p -- "$CFG_DIR" "$DADOS" "$CUSTOM" "$CASA/.local/state" "$CASA/.cache" "$CASA/tmp" \
        "$CASA/.claude" "$CASA/.codex" "$VAULT/.obsidian" "$VAULT/Notas" \
        "$VAULT/Projetos/Roteiro/05-backlog" "$OUTRA"

    local comandos topo sem_agentes falsos m
    comandos="$(cat <<EOF
[[command]]
name = "Data e hora"
argv = ["date"]

[[command]]
name = "Núcleo do sistema"
argv = ["uname", "-sr"]

[[command]]
name = "Fuso horário"
argv = ["date", "+%Z %z"]

[[command]]
name = "Texto longo"
argv = ["cat", $(toml "$TEXTO_LONGO")]
max_lines = 1000

[[command]]
name = "Números (lista longa)"
argv = ["seq", "1", "500"]
max_lines = 1000

[[command]]
name = "Processos do HUD"
argv = ["pgrep", "-a", "-f", $(toml "$CFG_DIR/config")]

[[command]]
name = "Espera 3 s"
argv = ["sleep", "3"]

[[command]]
name = "Tempo-limite"
argv = ["sleep", "30"]
timeout = 5

[[command]]
name = "Confirmação"
argv = ["echo", "confirmado"]
confirm = true

[[command]]
name = "Falha de propósito"
argv = ["false"]
EOF
)"
    topo="# Config de teste do roteiro do COSMIC (scripts/roteiro-cosmic.sh). Sem dados reais.
vault = $(toml "$VAULT")
data_dir = $(toml "$DADOS")
custom_dir = $(toml "$CUSTOM")
refresh_seconds = 1
vault_scan_seconds = 5
"
    sem_agentes=$'\n[claude]\nenabled = false\n\n[codex]\nenabled = false\n\n[opencode]\nenabled = false\n'
    # Agentes falsos: `false` sai com erro e a "resposta" é esse erro.
    falsos=""
    for m in claude codex opencode; do
        falsos+=$'\n'"[$m]"$'\nenabled = true\nexecutable = "false"\nprofile = "leitura"\ntimeout = 30\n'
    done
    printf '%s\n%s\n%s' "$topo" "$comandos" "$sem_agentes" > "${CFG[base]}"
    printf '%s\n%s\n%s' "$topo" "$comandos" "$falsos" > "${CFG[abas]}"

    printf -- '- [ ] %s 10:00 Item atrasado de teste\n- [ ] %s 09:00 Café da manhã: ação, pão\n- [ ] %s 14:00 Reunião ─│╭ caixas\n- [ ] %s Revisar emoji 🚀 sem hora\n' \
        "$(dia -1)" "$(dia 0)" "$(dia 1)" "$(dia 3)" > "$DADOS/agenda.md"

    printf '# Bem-vindo ao Vault de teste\n\nNota sintética do roteiro do HUD no COSMIC. Palavras para a busca: acentuação, coração, pão de queijo.\n\n- [ ] Tarefa do Vault 📅 %s ⏰ 16:00\n' \
        "$(dia 0)" > "$VAULT/Bem-vindo.md"
    local i
    for i in 1 2 3 4 5; do
        printf '# Nota %s\n\nTexto de exemplo %s com acentuação e símbolos ─│╭ █░ ●▶■.\n' "$i" "$i" > "$VAULT/Notas/Nota 0$i.md"
    done
    printf '# Roteiro\n\nProjeto de exemplo do roteiro de validação.\n' > "$VAULT/Projetos/Roteiro/Roteiro.md"
    cat > "$VAULT/Projetos/Roteiro/05-backlog/Kanban (Roteiro).md" <<'EOF'
---
kanban-plugin: board
---

## A fazer

- [ ] Conferir acentuação: ação, coração, pão
- [ ] Desenhar caixas ─│╭╮╰╯

## Em andamento

- [ ] Barras █░ e marcas ●▶■
- [ ] Lançar foguete 🚀

## Bloqueado

- [ ] Aguardando COSMIC de teste

## Concluído

- [x] Preparar dados sintéticos

%% kanban:settings
```
{"kanban-plugin":"board"}
```
%%
EOF
    printf '# Outra pasta\n\nPasta de exemplo para o /pasta.\n' > "$OUTRA/leia-me.md"
    for i in $(seq -w 1 300); do
        printf 'linha %s · ação, coração, pão ─│╭ █░ ●▶■ 🚀\n' "$i"
    done > "$TEXTO_LONGO"

    # Lançadores: abrem o HUD de teste com o perfil simulado na janela atual.
    for m in "${MODOS[@]}"; do
        cat > "$PASTA/abrir-hud-$m.sh" <<EOF
#!/usr/bin/env bash
# Gerado por scripts/roteiro-cosmic.sh: abre o HUD de teste (config $m).
cd $(q "$CASA") || exit 1
printf '\\033]0;HUD de teste ($m)\\007'
tamanho="\$(stty size 2>/dev/null || echo '? ?')"
printf 'HUD de teste (config $m). TERM=%s · %s colunas × %s linhas\\n' "\${TERM:-?}" "\${tamanho#* }" "\${tamanho% *}"
printf 'Feche com /sair; esta janela fica aberta para conferir a saída.\\n'
sleep 2
env HOME=$(q "$CASA") XDG_CONFIG_HOME=$(q "$CASA/.config") XDG_DATA_HOME=$(q "$CASA/.local/share") \\
    XDG_STATE_HOME=$(q "$CASA/.local/state") XDG_CACHE_HOME=$(q "$CASA/.cache") \\
    CLAUDE_CONFIG_DIR=$(q "$CASA/.claude") CODEX_HOME=$(q "$CASA/.codex") TMPDIR=$(q "$CASA/tmp") \\
    $(q "$HUD") -c $(q "${CFG[$m]}")
printf 'hud saiu com código %s\\n' "\$?"
EOF
        chmod 700 -- "$PASTA/abrir-hud-$m.sh"
    done
    chmod -R go-rwx -- "$PASTA"
}

JANELA_ABERTA=0
abrir_janela() {  # $1 = modo; abre uma janela nova do cosmic-term na pasta de teste
    local cmd="./abrir-hud-$1.sh"
    if command -v cosmic-term >/dev/null 2>&1; then
        # O cosmic-term 1.x só aceita -w (pasta de trabalho): o comando é colado.
        nohup cosmic-term -w "$PASTA" >/dev/null 2>&1 &
        JANELA_ABERTA=1
        printf '  janela nova do cosmic-term aberta em %s\n' "$PASTA"
    else
        printf '  cosmic-term não encontrado: abra um terminal e rode cd %s\n' "$(q "$PASTA")"
    fi
    colar "$cmd"
}

colar() {  # mostra o comando e, se der, põe na área de transferência
    printf '  cole na janela do HUD (Ctrl+Shift+V) e tecle Enter: %s\n' "$1"
    if command -v wl-copy >/dev/null 2>&1 && printf '%s' "$1" | wl-copy 2>/dev/null; then
        printf '  (já está na área de transferência)\n'
    fi
}

# ---------------------------------------------------------------- roteiro

IDS=() TITULOS=() SESSOES=() FACAS=() CONFIRAS=()
ler_roteiro() {
    local linha re_passo re_campo atual=-1 campo="" nome valor
    re_passo='^### (P[0-9]+) · (.+)$'
    re_campo='^- \*\*([^*:]+):\*\* ?(.*)$'
    while IFS= read -r linha || [[ -n $linha ]]; do
        if [[ $linha =~ $re_passo ]]; then
            IDS+=("${BASH_REMATCH[1]}"); TITULOS+=("${BASH_REMATCH[2]}")
            SESSOES+=(""); FACAS+=(""); CONFIRAS+=("")
            atual=$(( ${#IDS[@]} - 1 )); campo=""
            continue
        fi
        (( atual >= 0 )) || continue
        if [[ $linha == '##'* ]]; then atual=-1; continue; fi
        if [[ $linha =~ $re_campo ]]; then
            nome="${BASH_REMATCH[1]}"; valor="${BASH_REMATCH[2]}"; campo=""
            case "$nome" in
                Sess*) SESSOES[atual]="$valor"; campo=S ;;
                Fa*) FACAS[atual]="$valor"; campo=F ;;
                Conf*) CONFIRAS[atual]="$valor"; campo=C ;;
            esac
            continue
        fi
        if [[ -n $campo && $linha =~ ^[[:space:]]+[^[:space:]] ]]; then
            linha="${linha#"${linha%%[![:space:]]*}"}"
            case "$campo" in
                S) SESSOES[atual]+=" $linha" ;;
                F) FACAS[atual]+=" $linha" ;;
                C) CONFIRAS[atual]+=" $linha" ;;
            esac
        fi
    done < "$ROTEIRO"
}

modo_do_passo() {  # "abre o HUD (base)" -> base
    local re='abre o HUD \(([a-z]+)\)'
    if [[ $1 =~ $re ]]; then printf '%s' "${BASH_REMATCH[1]}"; fi
}

AMB_K=() AMB_V=()
amb() { AMB_K+=("$1"); AMB_V+=("$2"); }

coletar_ambiente() {
    local v so=""
    amb "Data" "$(date '+%Y-%m-%d %H:%M')"
    amb "hud" "$HUD"
    amb "hud --version" "$(hud_teste --version 2>&1 || true)"
    if command -v cosmic-term >/dev/null 2>&1; then
        v="$(cosmic-term --version 2>&1 | head -n 1 || true)"
    else
        v="(cosmic-term não encontrado)"
    fi
    amb "cosmic-term --version" "$v"
    amb "TERM (janela do condutor)" "${TERM:-(vazio)}"
    if [[ -t 0 ]] && v="$(stty size 2>/dev/null)"; then
        amb "Tamanho (janela do condutor)" "${v#* } colunas × ${v% *} linhas"
    else
        amb "Tamanho (janela do condutor)" "(sem terminal)"
    fi
    if [[ -r /etc/os-release ]]; then
        so="$(. /etc/os-release && printf '%s' "${PRETTY_NAME:-${NAME:-?}}")"
    fi
    amb "Sistema" "${so:-$(uname -sr)}"
    amb "Núcleo" "$(uname -sr)"
    amb "Sessão" "${XDG_SESSION_TYPE:-?} · ${XDG_CURRENT_DESKTOP:-?}"
    amb "Locale" "${LC_ALL:-${LANG:-?}}"
    amb "bash" "$BASH_VERSION"
}

R_ID=() R_TIT=() R_RES=() R_OBS=()
celula() { local s=${1//|/\\|}; printf '%s' "${s//$'\n'/ }"; }

gravar_relatorio() {
    local i n_ok=0 n_f=0 n_p=0 n_e=0
    for i in "${!R_RES[@]}"; do
        case "${R_RES[i]}" in ok) n_ok=$((n_ok+1)) ;; falhou) n_f=$((n_f+1)) ;;
            pulado) n_p=$((n_p+1)) ;; *) n_e=$((n_e+1)) ;; esac
    done
    {
        printf '# Roteiro da tela no COSMIC Terminal · %s\n\n' "${AMB_V[0]}"
        printf 'Gerado por `scripts/roteiro-cosmic.sh` a partir de `docs/roteiro-cosmic.md`. Dados sintéticos, HOME simulado.\n'
        if (( ENSAIO )); then printf '\n**Ensaio:** nada foi executado na tela; o relatório só prova o script.\n'; fi
        printf '\n## Ambiente\n\n| Item | Valor |\n|---|---|\n'
        for i in "${!AMB_K[@]}"; do printf '| %s | %s |\n' "${AMB_K[i]}" "$(celula "${AMB_V[i]}")"; done
        printf '\n## Resultados\n\n'
        printf '%s de %s passos registrados: %s ok, %s falhou, %s pulado, %s ensaio.\n\n' \
            "${#R_ID[@]}" "${#IDS[@]}" "$n_ok" "$n_f" "$n_p" "$n_e"
        printf '| Passo | Título | Resultado | Observação |\n|---|---|---|---|\n'
        for i in "${!R_ID[@]}"; do
            printf '| %s | %s | %s | %s |\n' "${R_ID[i]}" "$(celula "${R_TIT[i]}")" "${R_RES[i]}" "$(celula "${R_OBS[i]}")"
        done
    } > "$RELATORIO"
}

# ---------------------------------------------------------------- execução

ler_roteiro
(( ${#IDS[@]} > 0 )) || erro "nenhum passo (### Pnn) em $ROTEIRO"
for i in "${!IDS[@]}"; do
    m="$(modo_do_passo "${SESSOES[i]}")"
    if [[ -n $m && -z ${CFG[$m]+x} ]]; then erro "${IDS[i]}: config desconhecida '$m' (conhecidas: ${MODOS[*]})"; fi
done

IMPRESSAO_ANTES="$(impressao)"
nova_casa
printf 'pasta de teste: %s\n' "$PASTA"
for m in "${MODOS[@]}"; do
    printf '  config %s: %s\n' "$m" "${CFG[$m]}"
    printf '  abrir o HUD (%s) num terminal: cd %s && ./abrir-hud-%s.sh\n' "$m" "$(q "$PASTA")" "$m"
done
if (( SO_PREPARAR )); then exit 0; fi

coletar_ambiente
if (( ! ENSAIO )); then
    read -r -p 'Fonte do terminal (ex.: Noto Sans Mono 12; veja as Configurações do cosmic-term): ' fonte
    amb "Fonte do terminal" "$fonte"
fi
amb "Passos no roteiro" "${#IDS[@]} ($ROTEIRO)"

printf '\n== ambiente\n'
for i in "${!AMB_K[@]}"; do printf '  %s: %s\n' "${AMB_K[i]}" "${AMB_V[i]}"; done
printf '\n== hud --check -c %s\n' "${CFG[base]}"
set +e
saida_check="$(hud_teste --check -c "${CFG[base]}" 2>&1)"
codigo_check=$?
set -e
printf '%s\n(saiu com %s)\n' "$saida_check" "$codigo_check"
amb "hud --check (base)" "saiu com $codigo_check"

RELATORIO="$PASTA/roteiro-cosmic-$(date +%Y-%m-%d-%H%M).md"
ultimo_modo=base
for i in "${!IDS[@]}"; do
    printf '\n== %s · %s\n' "${IDS[i]}" "${TITULOS[i]}"
    printf '  Sessão:  %s\n  Faça:    %s\n  Confira: %s\n' "${SESSOES[i]}" "${FACAS[i]}" "${CONFIRAS[i]}"
    extra=""
    if [[ ${FACAS[i]} == *"linha de teste"* ]]; then printf '\n  linha de teste: %s\n' "$LINHA_TESTE"; fi
    if [[ ${CONFIRAS[i]} == *"pastas reais"* ]]; then
        pids="$(hud_rodando)"
        if [[ -z $pids ]]; then extra="nenhum hud de teste rodando"; else extra="hud de teste ainda rodando: pid $pids"; fi
        if [[ "$(impressao)" == "$IMPRESSAO_ANTES" ]]; then extra+="; pastas reais do HUD sem mudança"
        else extra+="; pastas reais do HUD mudaram desde o início (o seu HUD real estava aberto?)"; fi
        printf '  condutor: %s\n' "$extra"
    fi
    if (( ENSAIO )); then
        R_ID+=("${IDS[i]}"); R_TIT+=("${TITULOS[i]}"); R_RES+=(ensaio)
        R_OBS+=("${SESSOES[i]}${extra:+ [$extra]}")
        continue
    fi
    modo="$(modo_do_passo "${SESSOES[i]}")"
    if [[ -n $modo ]]; then
        ultimo_modo=$modo
        if (( JANELA_ABERTA )); then
            colar "./abrir-hud-$modo.sh"
            printf '  (use a janela do HUD que já está aberta; [a] abre outra)\n'
        else
            read -r -p "  Enter abre uma janela nova do cosmic-term para o HUD (config $modo) " _
            abrir_janela "$modo"
        fi
    fi
    if [[ ${FACAS[i]} == *"linha de teste"* ]] && command -v wl-copy >/dev/null 2>&1; then
        read -r -p '  Copiar a linha de teste para a área de transferência? (s/N) ' c
        if [[ $c == [sS]* ]]; then printf '%s' "$LINHA_TESTE" | wl-copy || printf '  não consegui copiar\n'; fi
    fi
    res=""
    while [[ -z $res ]]; do
        read -r -p '  Resultado: [o]k, [f]alhou, [p]ulado, [a]brir janela de novo, [s]air e gravar: ' r
        case "${r,,}" in
            o) res=ok ;;
            f) res=falhou ;;
            p) res=pulado ;;
            a) abrir_janela "$ultimo_modo" ;;
            s) res=parado ;;
            *) printf '  responda o, f, p, a ou s\n' ;;
        esac
    done
    if [[ $res == parado ]]; then break; fi
    read -r -p '  Observação (Enter para nenhuma): ' obs
    obs="${obs}${extra:+ [$extra]}"
    R_ID+=("${IDS[i]}"); R_TIT+=("${TITULOS[i]}"); R_RES+=("$res"); R_OBS+=("${obs# }")
    gravar_relatorio
done
gravar_relatorio
printf '\nrelatório: %s\n' "$RELATORIO"
printf 'anexe o relatório e as capturas à AT-012 (veja "Depois de executar" no roteiro).\n'
