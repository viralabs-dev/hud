#!/usr/bin/env bash
# Instalador do HUD para Linux e macOS: baixa o binário autocontido da release
# (não precisa de Python), confere o SHA-256 e instala em ~/.local/bin/hud.
# No Windows, use o install.ps1 (PowerShell).
#
#   curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash
#
# Variáveis:
#   HUD_VERSION      latest (padrão) ou uma tag vX.Y.Z
#   HUD_INSTALL_DIR  pasta de destino (padrão: ~/.local/bin)
#   HUD_REPOSITORY   repositório no GitHub (padrão: viralabs-dev/hud)
#
# Repetir a instalação atualiza. Não lê nada da entrada padrão.
set -euo pipefail

repo="${HUD_REPOSITORY:-viralabs-dev/hud}"
install_dir="${HUD_INSTALL_DIR:-$HOME/.local/bin}"
version="${HUD_VERSION:-latest}"

fail() { printf 'hud: %s\n' "$*" >&2; exit 1; }

case "$(uname -s)" in
  Linux) platform=linux ;;
  Darwin) platform=darwin ;;
  MINGW*|MSYS*|CYGWIN*) fail 'No Windows, use o PowerShell: irm https://raw.githubusercontent.com/viralabs-dev/hud/main/install.ps1 | iex' ;;
  *) fail "Sistema não suportado: $(uname -s) (há binários para Linux, macOS e Windows)." ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=amd64 ;;
  aarch64|arm64) arch=arm64 ;;
  *) fail "Arquitetura não suportada: $(uname -m) (há binários para amd64 e arm64)." ;;
esac
# Num Mac com Apple Silicon, um terminal sob Rosetta diz x86_64: prefira o nativo.
if [[ "$platform" == darwin && "$arch" == amd64 ]] &&
   [[ "$(/usr/sbin/sysctl -n sysctl.proc_translated 2>/dev/null || true)" == 1 ]]; then
  arch=arm64
fi
for tool in curl tar mktemp awk; do
  command -v "$tool" >/dev/null || fail "Requisito ausente: $tool"
done
[[ "$repo" =~ ^[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+$ ]] || fail 'HUD_REPOSITORY deve ser dono/repositório.'
if [[ "$version" != latest && ! "$version" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]]; then
  fail 'HUD_VERSION deve ser latest ou uma tag vX.Y.Z.'
fi
if [[ "$(id -u)" == 0 && -z "${HUD_INSTALL_DIR:-}" ]]; then
  fail 'Não rode o instalador como root: o HUD recusa rodar como root. Instale com o seu usuário.'
fi

asset="hud_${platform}_${arch}.tar.gz"
if [[ "$version" == latest ]]; then
  base="https://github.com/$repo/releases/latest/download"
else
  base="https://github.com/$repo/releases/download/$version"
fi

# Destino: recusa symlink antes de baixar qualquer coisa.
target="$install_dir/hud"
notice_dir="$install_dir/hud-licenses"
if [[ -L "$target" ]]; then
  fail "$(printf '%s é um symlink (aponta para %s).\n     O instalador não escreve através de symlink. É a instalação antiga;\n     remova o link e rode o instalador de novo:\n       rm %q\n     (se veio do pipx, use: pipx uninstall hud)' \
    "$target" "$(readlink "$target")" "$target")"
fi
[[ ! -e "$target" || -f "$target" ]] || fail "$target existe e não é um arquivo comum."
for p in "$notice_dir" "$notice_dir/LICENSES"; do
  [[ ! -L "$p" ]] || fail "$p é um symlink; remova-o ou escolha outro HUD_INSTALL_DIR."
done

tmp="$(mktemp -d)"
trap 'rm -rf -- "$tmp"' EXIT
dl() { curl --proto '=https' --tlsv1.2 -fsSL --retry 3 "$1" -o "$2" </dev/null; }
printf 'Baixando %s (%s)...\n' "$asset" "$version"
dl "$base/$asset" "$tmp/$asset" || fail "Falha ao baixar $base/$asset"
dl "$base/checksums.txt" "$tmp/checksums.txt" || fail "Falha ao baixar $base/checksums.txt"

expected="$(awk -v file="$asset" '$2 == file || $2 == "*" file { print $1 }' "$tmp/checksums.txt")"
[[ "$expected" =~ ^[0-9a-f]{64}$ ]] || fail 'Checksum ausente ou inválido em checksums.txt.'
if command -v sha256sum >/dev/null; then
  actual="$(sha256sum "$tmp/$asset")"
elif command -v shasum >/dev/null; then  # macOS: não tem sha256sum
  actual="$(shasum -a 256 "$tmp/$asset")"
else
  fail 'Instale sha256sum (coreutils) ou shasum.'
fi
[[ "${actual%% *}" == "$expected" ]] || fail 'Checksum divergente; instalação interrompida.'

# Só extrai os membros esperados, todos arquivos comuns.
notices=(THIRD_PARTY_NOTICES.md)
while IFS= read -r m; do
  case "$m" in
    LICENSE) notices+=("$m") ;;  # licença MIT do HUD (pacotes desde a v0.5.1)
    LICENSES/*.txt) [[ "$m" =~ ^LICENSES/[A-Za-z0-9._-]+\.txt$ ]] && notices+=("$m") ;;
  esac
done < <(tar -tzf "$tmp/$asset")
mkdir "$tmp/x"
tar -xzf "$tmp/$asset" -C "$tmp/x" --no-same-owner --no-same-permissions hud "${notices[@]}" ||
  fail 'Pacote incompleto: faltam o binário ou os avisos de licença.'
for f in hud "${notices[@]}"; do
  [[ -f "$tmp/x/$f" && ! -L "$tmp/x/$f" ]] || fail "Arquivo inválido no pacote: $f"
done

# Escrita atômica: copia para um temporário na mesma pasta e renomeia.
mkdir -p "$install_dir"
new="$(mktemp "$install_dir/.hud-install.XXXXXX")"
trap 'rm -rf -- "$tmp"; rm -f -- "$new"' EXIT
cp "$tmp/x/hud" "$new"
chmod 755 "$new"
if [[ "$platform" == darwin ]]; then
  # O curl não marca o download com quarentena, mas um pacote baixado pelo
  # navegador e passado por aqui (ou um proxy que marque) faria o Gatekeeper
  # barrar o binário, que não é assinado nem notarizado pela Apple.
  xattr -d com.apple.quarantine "$new" 2>/dev/null || true
  hint='macOS antigo?'
else
  hint='glibc antiga?'
fi
"$new" --version >/dev/null </dev/null || fail "O binário baixado não roda nesta máquina ($hint)."
[[ ! -L "$target" ]] || fail "$target virou symlink durante a instalação."
mv -f "$new" "$target"

mkdir -p "$notice_dir/LICENSES"
for f in "${notices[@]}"; do
  [[ ! -L "$notice_dir/$f" ]] || fail "Destino de licença é symlink: $notice_dir/$f"
  n="$(mktemp "$notice_dir/.tmp.XXXXXX")"
  cp "$tmp/x/$f" "$n"
  chmod 644 "$n"
  mv -f "$n" "$notice_dir/$f"
done

printf 'HUD instalado: %s (%s)\n' "$target" "$("$target" --version </dev/null)"
printf 'Avisos de licença: %s\n' "$notice_dir"
if [[ "$platform" == darwin ]]; then
  printf 'Nota: o binário não é assinado nem notarizado pela Apple. Instalado por aqui, ele roda;\n'
  printf '      se o macOS bloquear (arquivo vindo do navegador), rode: xattr -d com.apple.quarantine %q\n' "$target"
fi
case ":$PATH:" in
  *":$install_dir:"*) ;;
  *) printf 'Adicione ao PATH: export PATH="%s:$PATH"\n' "$install_dir" ;;
esac
