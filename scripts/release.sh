#!/usr/bin/env bash
# Gera o binário autocontido do HUD para a máquina atual e o pacote de release:
#   Linux   dist/hud_linux_<amd64|arm64>.tar.gz   (hud + LICENSE + THIRD_PARTY_NOTICES.md + LICENSES/)
#   macOS   dist/hud_darwin_<arm64|amd64>.tar.gz  (idem)
#   Windows dist/hud_windows_amd64.zip            (hud.exe + idem; rode no Git Bash)
#   dist/checksums.txt  (sha256 de todos os dist/hud_*; a release junta tudo num só)
#
# Uso: scripts/release.sh <tag>      ex.: scripts/release.sh v0.6.0
# Requer um Python 3.11+ com PyInstaller (packaging/requirements-build*.txt da
# plataforma); escolha qual com PYTHON=/caminho/do/python (padrão: python3).
# O PyInstaller não faz build cruzado: cada sistema e arquitetura é montado no próprio.
set -euo pipefail

tag="${1:-}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]] || {
  echo 'Uso: scripts/release.sh vX.Y.Z' >&2; exit 2; }

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
cd "$root"

version="$(sed -n 's/^__version__ = "\(.*\)"$/\1/p' hud/__init__.py | tr -d '\r')"
[[ "$tag" == "v$version" ]] || {
  echo "A tag $tag não bate com hud/__init__.py (__version__ = \"$version\")." >&2; exit 1; }

case "$(uname -s)" in
  Linux) os=linux; ext=tar.gz; exe=hud ;;
  Darwin) os=darwin; ext=tar.gz; exe=hud ;;
  MINGW*|MSYS*|CYGWIN*) os=windows; ext=zip; exe=hud.exe ;;
  *) echo "Sistema sem binário: $(uname -s)" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64|amd64) arch=amd64 ;;
  aarch64|arm64) arch=arm64 ;;
  *) echo "Arquitetura não suportada: $(uname -m)" >&2; exit 1 ;;
esac
[[ "$os" != windows || "$arch" == amd64 ]] || { echo 'No Windows, só amd64.' >&2; exit 1; }

if [[ "$os" == linux ]]; then
  command -v objdump >/dev/null || { echo 'Requisito ausente: objdump (pacote binutils), usado pelo PyInstaller.' >&2; exit 1; }
fi
python="${PYTHON:-python3}"
"$python" -I -c 'import sys, _curses, PyInstaller; assert sys.version_info >= (3, 11)' || {
  echo "Preciso de Python 3.11+ com _curses e PyInstaller em $python (ver packaging/requirements-build*.txt)." >&2
  exit 1; }
echo "Python: $("$python" -I -c 'import sys; print(sys.version.split()[0])') · PyInstaller: $("$python" -I -m PyInstaller --version) · $os/$arch"
if [[ "$os" == darwin ]]; then
  # O binário roda na arquitetura do Python que monta. Se o Python for
  # universal2 rodando sob Rosetta (ou o contrário), o pacote sairia trocado.
  py_arch="$("$python" -I -c 'import platform; print(platform.machine())')"
  case "$py_arch" in x86_64) py_arch=amd64 ;; arm64) py_arch=arm64 ;; esac
  [[ "$py_arch" == "$arch" ]] || { echo "Python em $py_arch numa máquina $arch." >&2; exit 1; }
fi

work="$root/build/pyinstaller"
rm -rf -- "$work"
"$python" -I -m PyInstaller --noconfirm --clean --log-level WARN \
  --distpath "$work/dist" --workpath "$work/work" packaging/hud.spec
bin="$work/dist/$exe"
[[ -f "$bin" ]] || { echo "PyInstaller não gerou $bin." >&2; exit 1; }

# Smoke do binário recém-montado, fora do repositório e com ambiente mínimo.
if [[ "$os" == windows ]]; then
  got="$(cd / && "$bin" --version | tr -d '\r')"
  [[ "$got" == "hud $version" ]] || { echo "Smoke falhou: '$got' != 'hud $version'." >&2; exit 1; }
  ps="$(command -v pwsh || command -v powershell)"
  "$ps" -NoProfile -NonInteractive -ExecutionPolicy Bypass -File packaging/smoke.ps1 "$(cygpath -w "$bin")"
else
  got="$(cd / && env -i PATH=/usr/bin:/bin HOME=/nonexistent "$bin" --version)"
  [[ "$got" == "hud $version" ]] || { echo "Smoke falhou: '$got' != 'hud $version'." >&2; exit 1; }
  bash packaging/smoke.sh "$bin"
fi

# Toda biblioteca nativa embutida precisa de licença em LICENSES/ (falha se não).
"$python" -I packaging/bibliotecas.py "$bin"
# glibc mínimo exigido pelo binário e pelas bibliotecas embutidas (informativo).
[[ "$os" != linux ]] || "$python" -I packaging/glibc_minimo.py "$bin" || true

# Pacote reproduzível (ordem, dono e datas fixos) e dist/checksums.txt.
epoch="$(git log -1 --format=%ct 2>/dev/null || echo 0)"
asset="hud_${os}_${arch}.${ext}"
mkdir -p dist
rm -f -- "dist/$asset"
"$python" -I packaging/empacotar.py "$bin" "dist/$asset" "$epoch"
echo "Binário: $(du -h "$bin" | cut -f1)"
