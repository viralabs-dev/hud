#!/usr/bin/env bash
# Smoke do binário autocontido, fora do repositório e numa pasta temporária:
#   1. hud --version
#   2. hud --check -c <config 600> (Claude e Codex desligados, sai com 0)
#   3. isolamento: módulos plantados (re, json, curses, os, sitecustomize,
#      encodings, hud...) no diretório atual e em PYTHONPATH/PYTHONHOME/
#      PYTHONSTARTUP não podem ser carregados. Se algum for, ele grava uma
#      marca e o teste falha.
# Uso: packaging/smoke.sh caminho/do/hud      (Linux e macOS; no Windows, smoke.ps1)
set -euo pipefail

arg="${1:?uso: packaging/smoke.sh caminho/do/hud}"
bin="$(cd "$(dirname "$arg")" && pwd -P)/$(basename "$arg")"  # sem readlink -f (macOS antigo)
[[ -x "$bin" ]] || { echo "Não executável: $bin" >&2; exit 1; }
dir="$(mktemp -d)"
trap 'rm -rf -- "$dir"' EXIT
chmod 700 "$dir"
mark="$dir/MARCA"
mkdir -p "$dir/home" "$dir/vault"

echo "== hud --version"
(cd "$dir" && env -i PATH=/usr/bin:/bin HOME="$dir/home" "$bin" --version)

cfg="$dir/config.toml"
cat > "$cfg" <<EOF
vault = "$dir/vault"
data_dir = "$dir/dados"

[[command]]
name = "Eco"
argv = ["echo", "ok"]

[claude]
enabled = false

[codex]
enabled = false
EOF
chmod 600 "$cfg"
echo "== hud --check"
(cd "$dir" && env -i PATH=/usr/bin:/bin HOME="$dir/home" "$bin" --check -c "$cfg")

echo "== modelos e skill embutidos"
out="$(cd "$dir" && env -i PATH=/usr/bin:/bin HOME="$dir/home" "$bin" --custom-check foco -c "$cfg")"
grep -q '^OK: foco ' <<<"$out" || { echo "$out" >&2; echo 'FALHOU: o modelo foco não veio no binário' >&2; exit 1; }
mkdir -p "$dir/home/.claude"
out="$(cd "$dir" && env -i PATH=/usr/bin:/bin HOME="$dir/home" "$bin" --instalar-skill)" ||
  { echo "$out" >&2; echo 'FALHOU: hud --instalar-skill saiu com erro' >&2; exit 1; }
echo "$out"
grep -q '^name: hud-custom' "$dir/home/.claude/skills/hud-custom/SKILL.md" ||
  { echo 'FALHOU: hud --instalar-skill não instalou a skill hud-custom' >&2; exit 1; }
# Toda skill embutida: uma linha "<destino>: instalada" por skill no Claude, cada
# destino com SKILL.md, e nenhuma pasta a mais ou a menos em .claude/skills.
skills="$dir/home/.claude/skills"
n=0
while IFS= read -r linha; do
  destino="${linha%: instalada}"
  [ "$(dirname "$destino")" = "$skills" ] || continue
  [ -f "$destino/SKILL.md" ] || { echo "FALHOU: $destino sem SKILL.md" >&2; exit 1; }
  n=$((n + 1))
done < <(grep ': instalada$' <<<"$out")
pastas="$(find "$skills" -mindepth 1 -maxdepth 1 -type d | wc -l)"
[ "$n" -ge 1 ] && [ "$n" -eq "$pastas" ] ||
  { echo "FALHOU: $n skills instaladas, $pastas pastas em .claude/skills" >&2; exit 1; }
if grep -v ': instalada$' <<<"$out" | grep -q "^$skills/"; then
  echo 'FALHOU: alguma skill não foi instalada no Claude' >&2; exit 1
fi
echo "   $n skills instaladas"
for f in projeto-docs/SKILL.md projeto-docs/modelos/kanban.md; do
  [ -f "$skills/$f" ] || { echo "FALHOU: a skill projeto-docs veio sem $f" >&2; exit 1; }
done

echo "== isolamento (módulos plantados)"
for mod in re json curses os sys locale argparse pathlib tomllib subprocess threading \
           sitecustomize usercustomize; do
  printf 'open("%s", "a").write("%s carregado\\n")\nraise SystemExit("PLANTADO: %s")\n' \
    "$mark" "$mod" "$mod" > "$dir/$mod.py"
done
mkdir -p "$dir/hud" "$dir/encodings" "$dir/lib"
cp "$dir/re.py" "$dir/hud/__init__.py"
cp "$dir/re.py" "$dir/hud/__main__.py"
cp "$dir/re.py" "$dir/encodings/__init__.py"
for v in 3.11 3.12 3.13 3.14; do mkdir -p "$dir/lib/python$v"; cp "$dir/re.py" "$dir/lib/python$v/os.py"; done
printf 'open("%s", "a").write("PYTHONSTARTUP carregado\\n")\n' "$mark" > "$dir/startup.py"

for vars in \
  "PYTHONPATH=$dir" \
  "PYTHONHOME=$dir" \
  "PYTHONPATH=$dir PYTHONHOME=$dir PYTHONSTARTUP=$dir/startup.py PYTHONINSPECT=1 PYTHONUSERBASE=$dir PYTHONSAFEPATH=0" \
  "PYTHONVERBOSE=1 PYTHONWARNINGS=error PYTHONDEBUG=1"; do
  echo "   $vars"
  # shellcheck disable=SC2086  # vars é uma lista de VAR=valor sem espaços nos valores
  out="$(cd "$dir" && env -i PATH=/usr/bin:/bin HOME="$dir/home" $vars "$bin" --check -c "$cfg" 2>&1)" || {
    echo "$out" >&2; echo "FALHOU: hud --check saiu com erro com $vars" >&2; exit 1; }
  # PYTHONVERBOSE=1 faria o Python listar cada import ("import ..."/"# ...").
  if grep -Eq '^(import |# )' <<<"$out"; then
    echo "$out" >&2; echo "FALHOU: variável PYTHON* respeitada ($vars)" >&2; exit 1
  fi
done
if [[ -e "$mark" ]]; then
  cat "$mark" >&2; echo 'FALHOU: módulo plantado foi carregado.' >&2; exit 1
fi
echo "   nenhum módulo plantado foi carregado"

# Controle: prova que a armadilha funciona num Python comum sem -I.
if command -v python3 >/dev/null; then
  mkdir "$dir/controle"; cp "$dir/json.py" "$dir/controle/json.py"
  (cd / && PYTHONPATH="$dir/controle" python3 -c 'import json' 2>/dev/null) || true
  if [[ -e "$mark" ]]; then
    echo "   controle: python3 sem -I carregou o json plantado (armadilha válida)"
  else
    echo 'FALHOU: o controle não carregou o módulo plantado; o teste não prova nada.' >&2; exit 1
  fi
fi
echo "smoke ok"
