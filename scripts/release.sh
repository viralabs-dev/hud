#!/usr/bin/env bash
# Gera o binário autocontido do HUD para a máquina atual (linux amd64 ou arm64)
# e o pacote de release:
#   dist/hud_linux_<arch>.tar.gz  (hud + LICENSE + THIRD_PARTY_NOTICES.md + LICENSES/)
#   dist/checksums.txt            (sha256 de todos os dist/hud_*.tar.gz)
#
# Uso: scripts/release.sh <tag>      ex.: scripts/release.sh v0.4.0
# Requer um Python 3.11+ com PyInstaller (packaging/requirements-build.txt);
# escolha qual com PYTHON=/caminho/do/python (padrão: python3).
# O PyInstaller não faz build cruzado: cada arquitetura é montada na própria.
set -euo pipefail

tag="${1:-}"
[[ "$tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+([.-][A-Za-z0-9.-]+)?$ ]] || {
  echo 'Uso: scripts/release.sh vX.Y.Z' >&2; exit 2; }

root="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^__version__ = "\(.*\)"$/\1/p' hud/__init__.py)"
[[ "$tag" == "v$version" ]] || {
  echo "A tag $tag não bate com hud/__init__.py (__version__ = \"$version\")." >&2; exit 1; }

[[ "$(uname -s)" == Linux ]] || { echo 'O binário só é montado em Linux.' >&2; exit 1; }
case "$(uname -m)" in
  x86_64|amd64) arch=amd64 ;;
  aarch64|arm64) arch=arm64 ;;
  *) echo "Arquitetura não suportada: $(uname -m)" >&2; exit 1 ;;
esac

command -v objdump >/dev/null || { echo 'Requisito ausente: objdump (pacote binutils), usado pelo PyInstaller.' >&2; exit 1; }
python="${PYTHON:-python3}"
"$python" -I -c 'import sys, _curses, PyInstaller; assert sys.version_info >= (3, 11)' || {
  echo "Preciso de Python 3.11+ com _curses e PyInstaller em $python (ver packaging/requirements-build.txt)." >&2
  exit 1; }
echo "Python: $("$python" -I -c 'import sys; print(sys.version.split()[0])') · PyInstaller: $("$python" -I -m PyInstaller --version)"

work="$root/build/pyinstaller"
rm -rf -- "$work"
"$python" -I -m PyInstaller --noconfirm --clean --log-level WARN \
  --distpath "$work/dist" --workpath "$work/work" packaging/hud.spec
bin="$work/dist/hud"

# Smoke do binário recém-montado, fora do repositório e com ambiente mínimo.
got="$(cd / && env -i PATH=/usr/bin:/bin HOME=/nonexistent "$bin" --version)"
[[ "$got" == "hud $version" ]] || { echo "Smoke falhou: '$got' != 'hud $version'." >&2; exit 1; }
bash packaging/smoke.sh "$bin"

# glibc mínimo exigido pelo binário e pelas bibliotecas embutidas (informativo).
"$python" -I packaging/glibc_minimo.py "$bin" || true

asset="hud_linux_${arch}.tar.gz"
stage="$work/stage"
mkdir -p "$stage/LICENSES" dist
install -m 755 "$bin" "$stage/hud"
install -m 644 LICENSE THIRD_PARTY_NOTICES.md "$stage/"
install -m 644 LICENSES/*.txt "$stage/LICENSES/"
# Pacote reproduzível: ordem, dono e datas fixos.
epoch="$(git log -1 --format=%ct 2>/dev/null || echo 0)"
(cd "$stage" && find . -mindepth 1 -printf '%P\n' | LC_ALL=C sort |
  tar --no-recursion --owner=0 --group=0 --numeric-owner --mtime="@$epoch" -cf - -T - |
  gzip -n -9 > "$root/dist/$asset")

(cd dist && sha256sum hud_*.tar.gz > checksums.txt)
echo "Gerado: dist/$asset ($(du -h "dist/$asset" | cut -f1); binário $(du -h "$bin" | cut -f1))"
cat dist/checksums.txt
