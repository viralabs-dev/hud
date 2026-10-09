#!/usr/bin/env bash
# Testa o install.sh sem rede e sem tocar no ~/.local/bin: um `curl` falso no
# PATH serve os arquivos de uma "release" local (e confere que o instalador só
# pede HTTPS com TLS 1.2+). Tudo acontece numa pasta temporária.
#
# Roda no Linux e no macOS (bash 3.2, ferramentas BSD).
#
# Uso: scripts/test-installer.sh [pacote.tar.gz]
#   Sem argumento, monta um pacote falso (binário = script que imprime a versão).
#   Com um dist/hud_<linux|darwin>_<arch>.tar.gz real, instala o binário de verdade.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
work="$(mktemp -d)"
trap 'rm -rf -- "$work"' EXIT
chmod 700 "$work"

case "$(uname -s)" in Linux) os=linux ;; Darwin) os=darwin ;; *) echo 'sistema sem teste (no Windows: scripts/test-installer.ps1)' >&2; exit 1 ;; esac
case "$(uname -m)" in x86_64|amd64) arch=amd64 ;; aarch64|arm64) arch=arm64 ;; *) echo 'arquitetura sem teste' >&2; exit 1 ;; esac
if [[ "$os" == darwin && "$(/usr/sbin/sysctl -n sysctl.proc_translated 2>/dev/null || true)" == 1 ]]; then
  arch=arm64  # o instalador também prefere o nativo sob Rosetta
fi
asset="hud_${os}_${arch}.tar.gz"
sha256() { if command -v sha256sum >/dev/null; then sha256sum "$@"; else shasum -a 256 "$@"; fi; }
mode_of() { stat -c %a "$1" 2>/dev/null || stat -f %Lp "$1"; }
repo="teste/hud"
pass=0
ok() { pass=$((pass + 1)); printf 'ok %d - %s\n' "$pass" "$1"; }
die() { printf 'FALHOU: %s\n' "$1" >&2; [[ -f "$work/out" ]] && sed 's/^/  | /' "$work/out" >&2; exit 1; }

# ── "releases" locais ────────────────────────────────────────────────────
make_release() { # make_release <tag> <versão> [pacote real]
  local dir="$work/srv/$repo/releases/download/$1" stage="$work/stage-$1"
  mkdir -p "$dir" "$stage/LICENSES"
  if [[ -n "${3:-}" ]]; then
    cp "$3" "$dir/$asset"
  else
    printf '#!/bin/sh\necho "hud %s"\n' "$2" > "$stage/hud"
    chmod 755 "$stage/hud"
    cp "$root/LICENSE" "$root/THIRD_PARTY_NOTICES.md" "$stage/"
    cp "$root"/LICENSES/*.txt "$stage/LICENSES/"
    if [[ "$1" == v0.1.0 ]]; then  # pacote antigo, sem LICENSE: continua instalável
      tar -czf "$dir/$asset" -C "$stage" hud THIRD_PARTY_NOTICES.md LICENSES
    else
      tar -czf "$dir/$asset" -C "$stage" hud LICENSE THIRD_PARTY_NOTICES.md LICENSES
    fi
  fi
  (cd "$dir" && sha256 "$asset" > checksums.txt)
}
make_release v0.1.0 0.1.0
make_release v0.2.0 0.2.0 "${1:-}"
latest_v="hud 0.2.0"
if [[ -n "${1:-}" ]]; then  # pacote real: a versão é a do binário dentro dele
  mkdir "$work/real"; tar -xzf "$1" -C "$work/real" hud
  latest_v="$("$work/real/hud" --version)"
fi
mkdir -p "$work/srv/$repo/releases/latest"
ln -s ../download/v0.2.0 "$work/srv/$repo/releases/latest/download"
mkdir -p "$work/srv/$repo/releases/download/v0.3.0"
cp "$work/srv/$repo/releases/download/v0.2.0/"* "$work/srv/$repo/releases/download/v0.3.0/"
printf '%064d  %s\n' 0 "$asset" > "$work/srv/$repo/releases/download/v0.3.0/checksums.txt"  # checksum errado
mkdir -p "$work/srv/$repo/releases/download/v0.4.0"
cp "$work/srv/$repo/releases/download/v0.2.0/$asset" "$work/srv/$repo/releases/download/v0.4.0/"
echo 'sem nada útil' > "$work/srv/$repo/releases/download/v0.4.0/checksums.txt"  # checksum ausente

# ── curl falso ───────────────────────────────────────────────────────────
mkdir "$work/fakebin"
cat > "$work/fakebin/curl" <<EOF
#!/usr/bin/env bash
# Exige --proto '=https' e --tlsv1.2; serve https://github.com/... de $work/srv.
set -euo pipefail
proto=0 tls=0 out= url=
while (( \$# )); do
  case "\$1" in
    --proto) [[ "\$2" == '=https' ]] && proto=1; shift ;;
    --tlsv1.2) tls=1 ;;
    -o) out="\$2"; shift ;;
    --retry) shift ;;
    -*) ;;
    *) url="\$1" ;;
  esac
  shift
done
[[ \$proto == 1 && \$tls == 1 ]] || { echo "curl falso: faltou --proto =https/--tlsv1.2" >&2; exit 90; }
[[ "\$url" == https://github.com/* ]] || { echo "curl falso: URL inesperada \$url" >&2; exit 91; }
echo "\$url" >> "$work/curl.log"
src="$work/srv/\${url#https://github.com/}"
[[ -f "\$src" ]] || { echo "curl: (22) 404 \$url" >&2; exit 22; }
cp "\$src" "\$out"
EOF
chmod 755 "$work/fakebin/curl"

home="$work/home"
mkdir -p "$home"
dest="$home/.local/bin"
run() { # run VAR=valor... ; roda o instalador como no `curl | bash`
  cat "$root/install.sh" | env -i PATH="$work/fakebin:/usr/bin:/bin" HOME="$home" \
    HUD_REPOSITORY="$repo" "$@" bash > "$work/out" 2>&1
}

# 1. latest
run || die 'instalação latest'
[[ "$("$dest/hud" --version)" == "$latest_v" ]] || die "latest não instalou $latest_v"
[[ -f "$dest/hud-licenses/THIRD_PARTY_NOTICES.md" ]] || die 'avisos não instalados'
cmp -s "$root/LICENSE" "$dest/hud-licenses/LICENSE" || die 'licença MIT do HUD não instalada'
for f in "$root"/LICENSES/*.txt; do
  cmp -s "$f" "$dest/hud-licenses/LICENSES/$(basename "$f")" || die "licença ausente: $(basename "$f")"
done
grep -q 'Adicione ao PATH' "$work/out" || die 'não avisou do PATH'
grep -q 'releases/latest/download/' "$work/curl.log" || die 'latest não usou releases/latest'
[[ "$(mode_of "$dest/hud")" == 755 ]] || die 'permissão do binário'
grep -q "/$asset\$" "$work/curl.log" || die "não baixou $asset"
if [[ "$os" == darwin ]]; then
  grep -q 'não é assinado nem notarizado' "$work/out" || die 'não avisou do Gatekeeper'
  if xattr -p com.apple.quarantine "$dest/hud" >/dev/null 2>&1; then die 'binário ficou em quarentena'; fi
else
  if grep -q 'notarizado' "$work/out"; then die 'aviso do Gatekeeper fora do macOS'; fi
fi
ok 'instala a latest com binário e avisos de licença'

# 2. versão fixa (volta para a 0.1.0) e repetição (atualiza para a 0.2.0)
run HUD_VERSION=v0.1.0 || die 'instalação v0.1.0'
[[ "$("$dest/hud" --version)" == "hud 0.1.0" ]] || die 'HUD_VERSION=v0.1.0 não instalou a 0.1.0'
run HUD_VERSION=v0.2.0 || die 'atualização'
[[ "$("$dest/hud" --version)" == "$latest_v" ]] || die 'repetir não atualizou'
[[ -z "$(find "$dest" -maxdepth 2 \( -name '.hud-install.*' -o -name '.tmp.*' \))" ]] || die 'sobrou temporário'
ok 'HUD_VERSION fixa e repetição atualizam sem sobrar temporário'

# 3. HUD_INSTALL_DIR
run HUD_INSTALL_DIR="$work/outro" || die 'HUD_INSTALL_DIR'
[[ -x "$work/outro/hud" && -f "$work/outro/hud-licenses/THIRD_PARTY_NOTICES.md" ]] || die 'HUD_INSTALL_DIR ignorado'
ok 'HUD_INSTALL_DIR'

# 4. checksum divergente e ausente: não toca no binário instalado
before="$(sha256 "$dest/hud")"
if run HUD_VERSION=v0.3.0; then die 'aceitou checksum divergente'; fi
grep -q 'Checksum divergente' "$work/out" || die 'mensagem de checksum divergente'
if run HUD_VERSION=v0.4.0; then die 'aceitou checksum ausente'; fi
grep -q 'Checksum ausente' "$work/out" || die 'mensagem de checksum ausente'
[[ "$(sha256 "$dest/hud")" == "$before" ]] || die 'binário mudou após checksum inválido'
ok 'checksum divergente ou ausente interrompe sem mexer no instalado'

# 5. versão inválida e versão inexistente
for v in 1.2.3 'v1.2' 'v1.2.3;rm' '../v1.2.3' 'latest/../x'; do
  if run HUD_VERSION="$v"; then die "aceitou HUD_VERSION=$v"; fi
  grep -q 'HUD_VERSION deve ser' "$work/out" || die "mensagem para HUD_VERSION=$v"
done
if run HUD_VERSION=v9.9.9; then die 'aceitou versão inexistente'; fi
grep -q 'Falha ao baixar' "$work/out" || die 'mensagem de versão inexistente'
if run HUD_REPOSITORY='a/b/../c'; then die 'aceitou HUD_REPOSITORY inválido'; fi
ok 'versão ou repositório inválidos e versão inexistente são recusados'

# 6. destino symlink (o caso do link antigo para o repositório)
mkdir -p "$work/repo/bin"; printf '#!/bin/sh\necho antigo\n' > "$work/repo/bin/hud"; chmod 755 "$work/repo/bin/hud"
rm -f "$dest/hud"; ln -s "$work/repo/bin/hud" "$dest/hud"
if run; then die 'escreveu através de symlink'; fi
grep -q "rm .*$dest/hud" "$work/out" || die 'não disse como remover o symlink'
sed 's/^/   | /' "$work/out"
[[ "$("$work/repo/bin/hud")" == antigo && -L "$dest/hud" ]] || die 'mexeu no alvo do symlink'
rm "$dest/hud"
run || die 'instalação depois de remover o symlink'
[[ ! -L "$dest/hud" && "$("$dest/hud" --version)" == "$latest_v" ]] || die 'não instalou depois do rm'
mv "$dest/hud-licenses" "$work/lic-real"; ln -s "$work/lic-real" "$dest/hud-licenses"
if run; then die 'escreveu avisos através de symlink'; fi
rm "$dest/hud-licenses"; mv "$work/lic-real" "$dest/hud-licenses"
ok 'destino symlink (binário ou avisos) é recusado com instrução de rm'

# 7. não lê a entrada do pipe: o que vier depois do script não é consumido
{ cat "$root/install.sh"; printf '\necho LINHA-EXTRA-EXECUTADA\n'; } > "$work/pipe.sh"
cat "$work/pipe.sh" | env -i PATH="$work/fakebin:/usr/bin:/bin" HOME="$home" HUD_REPOSITORY="$repo" \
  bash > "$work/out" 2>&1 || die 'instalação via pipe'
grep -q 'LINHA-EXTRA-EXECUTADA' "$work/out" || die 'algum comando consumiu a entrada do pipe'
ok 'nada consome o stdin do curl | bash'

# 8. sintaxe
bash -n "$root/install.sh" || die 'bash -n install.sh'
ok 'bash -n install.sh'

# 9. macOS simulado no Linux: uname diz Darwin, não há sha256sum (só shasum) e
#    o xattr é falso. Prova o ramo do macOS (pacote darwin, shasum, quarentena,
#    aviso do Gatekeeper) sem um Mac; o runner macos-* roda os grupos 1-8 de verdade.
if [[ "$os" == linux ]] && command -v shasum >/dev/null; then
  mac="$work/macbin"; mkdir "$mac"
  for t in bash sh cat env awk tar gzip mktemp cp chmod mv mkdir rm id readlink shasum perl sed grep; do
    p="$(command -v "$t")" || die "falta $t para o teste do macOS simulado"
    ln -s "$p" "$mac/$t"
  done
  cp "$work/fakebin/curl" "$mac/curl"
  for m in arm64 x86_64; do
    printf '#!/bin/sh\ncase "$1" in -s) echo Darwin ;; -m) echo %s ;; *) echo Darwin ;; esac\n' "$m" > "$mac/uname"
    printf '#!/bin/sh\necho "xattr $*" >> "%s/xattr.log"\nexit 1\n' "$work" > "$mac/xattr"
    chmod 755 "$mac/uname" "$mac/xattr"
    case "$m" in arm64) a=arm64 ;; *) a=amd64 ;; esac
    rel="$work/srv/$repo/releases/download/v0.5.0"
    rm -rf "$rel"; mkdir -p "$rel"
    cp "$work/srv/$repo/releases/download/v0.2.0/$asset" "$rel/hud_darwin_$a.tar.gz"
    (cd "$rel" && shasum -a 256 "hud_darwin_$a.tar.gz" > checksums.txt)
    rm -f "$work/xattr.log" "$work/curl.log"
    cat "$root/install.sh" | env -i PATH="$mac" HOME="$home" HUD_REPOSITORY="$repo" \
      HUD_VERSION=v0.5.0 HUD_INSTALL_DIR="$work/mac-$a" bash > "$work/out" 2>&1 || die "macOS simulado ($m)"
    grep -q "/hud_darwin_$a.tar.gz\$" "$work/curl.log" || die "macOS $m não baixou hud_darwin_$a.tar.gz"
    grep -q 'xattr -d com.apple.quarantine' "$work/xattr.log" || die 'macOS: não tirou a quarentena'
    grep -q 'não é assinado nem notarizado' "$work/out" || die 'macOS: sem aviso do Gatekeeper'
    [[ -x "$work/mac-$a/hud" && -f "$work/mac-$a/hud-licenses/THIRD_PARTY_NOTICES.md" ]] || die "macOS $m: não instalou"
  done
  printf '#!/bin/sh\ncase "$1" in -m) echo x86_64 ;; *) echo MINGW64_NT-10.0 ;; esac\n' > "$mac/uname"
  if cat "$root/install.sh" | env -i PATH="$mac" HOME="$home" bash > "$work/out" 2>&1; then die 'aceitou o bash do Windows'; fi
  grep -q 'install.ps1' "$work/out" || die 'bash do Windows: não indicou o install.ps1'
  ok 'macOS simulado (arm64 e x86_64: shasum, quarentena, aviso) e bash do Windows recusado'
fi

echo "instalador: $pass grupos de teste ok"
