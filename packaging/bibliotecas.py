"""Lista as bibliotecas nativas embutidas no binário e confere as licenças.

Lê o arquivo do PyInstaller (onefile) e classifica cada biblioteca ou extensão
nativa (ELF, Mach-O ou PE) por nome. Cada uma precisa cair numa regra com o
texto de licença em LICENSES/ (ou numa regra de componente do sistema, sem
texto). Uma biblioteca desconhecida faz o script sair com 1: alguém precisa
atualizar THIRD_PARTY_NOTICES.md, LICENSES/ e as regras abaixo antes da release.

Uso (com o Python do venv de build, que tem o PyInstaller):
    python -I packaging/bibliotecas.py build/pyinstaller/dist/hud[.exe] [-v]
"""

import os
import re
import struct
import sys

from PyInstaller.archive.readers import CArchiveReader

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))

# (regex do nome, sem a pasta, minúsculo) -> textos em LICENSES/ ("" = sem texto).
# A ordem importa: a primeira regra que casa vale.
REGRAS = [
    # windows-curses: _curses/_curses_panel com o PDCurses estático.
    (r"_curses(_panel)?(\.cp3\d+-win_(amd64|arm64))?\.pyd", ("windows-curses.txt", "pdcurses.txt")),
    # CPython: interpretador e extensões da biblioteca padrão.
    (r"libpython3\.\d+\.so(\.[\d.]+)?|libpython3\.\d+\.dylib|python3\d*\.dll|python", ("python.txt", "python-componentes.txt")),
    (r"[a-z0-9_]+\.cpython-3\d+[a-z0-9_-]*\.so", ("python.txt", "python-componentes.txt")),
    (r"[a-z0-9_]+(\.cp3\d+-win_(amd64|arm64))?\.pyd", ("python.txt", "python-componentes.txt")),
    # ncurses (Linux: do sistema; macOS: o que vem com o Python.org).
    (r"lib(ncursesw?|tinfo|panelw?)(\.\d+)*\.(so|dylib)(\.[\d.]+)?", ("ncurses.txt",)),
    (r"libz(\.\d+)*\.(so|dylib)(\.[\d.]+)?", ("zlib.txt",)),
    (r"libffi(-\d+)?(\.\d+)*\.(so|dylib|dll)(\.[\d.]+)?", ("libffi.txt",)),
    # Runtime do Visual C++ e UCRT da Microsoft (redistribuíveis; ver THIRD_PARTY_NOTICES.md).
    (r"vcruntime140(_1)?\.dll|msvcp140(_\d+)?\.dll|ucrtbase\.dll|api-ms-win-[a-z0-9-]+\.dll", ("",)),
]

MAGICS = (
    b"\x7fELF",                                      # Linux
    b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe",        # Mach-O 64/32 (little endian)
    b"\xca\xfe\xba\xbe",                             # Mach-O universal
    b"MZ",                                           # Windows PE
)


# Extensões de módulo do Python (.so do lib-dynload, .pyd); o resto é biblioteca.
EXTENSAO = re.compile(r"(\.cpython-3\d+[^/\\]*\.so|\.pyd)$", re.IGNORECASE)


def macos_minimo(data: bytes):
    """Maior versão mínima de macOS (LC_BUILD_VERSION/LC_VERSION_MIN_MACOSX) no Mach-O."""
    try:
        return _macos_minimo(data)
    except struct.error:  # cabeçalho cortado: só informativo
        return None


def _macos_minimo(data: bytes):
    fatias = []
    if data[:4] == b"\xca\xfe\xba\xbe":  # universal: big endian
        (n,) = struct.unpack_from(">I", data, 4)
        for i in range(n):
            _, _, off, size, _ = struct.unpack_from(">iiIII", data, 8 + 20 * i)
            fatias.append(data[off:off + size])
    else:
        fatias.append(data)
    maior = None
    for f in fatias:
        if f[:4] not in (b"\xcf\xfa\xed\xfe", b"\xce\xfa\xed\xfe"):
            continue
        ncmds = struct.unpack_from("<I", f, 16)[0]
        pos = 32 if f[:4] == b"\xcf\xfa\xed\xfe" else 28
        for _ in range(ncmds):
            cmd, size = struct.unpack_from("<II", f, pos)
            v = None
            if cmd == 0x32:    # LC_BUILD_VERSION: platform, minos, sdk...
                v = struct.unpack_from("<I", f, pos + 12)[0]
            elif cmd == 0x24:  # LC_VERSION_MIN_MACOSX: version, sdk
                v = struct.unpack_from("<I", f, pos + 8)[0]
            if v is not None:
                t = (v >> 16, (v >> 8) & 0xFF)
                maior = t if maior is None or t > maior else maior
            pos += size
    return maior


def classificar(nome: str):
    base = nome.replace("\\", "/").rsplit("/", 1)[-1].lower()
    for padrao, textos in REGRAS:
        if re.fullmatch(padrao, base):
            return textos
    return None


def main() -> int:
    if len(sys.argv) < 2:
        print(__doc__, file=sys.stderr)
        return 2
    path, verbose = sys.argv[1], "-v" in sys.argv[2:]
    arch = CArchiveReader(path)
    achadas, faltam, sem_regra = [], set(), []
    with open(path, "rb") as fh:
        cabeca = fh.read(1 << 20)
    macos = [("(bootloader)", macos_minimo(cabeca))] if cabeca.startswith(MAGICS[1:4]) else []
    for name, entry in arch.toc.items():
        if entry[-1] in ("o", "s", "m", "M", "z", "Z", "n", "l"):  # opção, código, PYZ, symlink, splash
            continue
        data = arch.extract(name)
        if not data or not data.startswith(MAGICS):
            continue
        textos = classificar(name)
        if textos is None:
            sem_regra.append(name)
            continue
        achadas.append((name, textos))
        if data.startswith(MAGICS[1:4]):
            macos.append((name, macos_minimo(data)))
        for t in textos:
            if t and not os.path.isfile(os.path.join(ROOT, "LICENSES", t)):
                faltam.add(t)
    if verbose:
        for name, textos in sorted(achadas):
            print(f"  {name:<48} {', '.join(t or '(sistema/Microsoft)' for t in textos)}")
    exts = sum(1 for n, _ in achadas if EXTENSAO.search(n))
    libs = sorted(n.replace("\\", "/").rsplit("/", 1)[-1] for n, _ in achadas if not EXTENSAO.search(n))
    print(f"bibliotecas embutidas ({exts} extensões .so/.pyd à parte):", ", ".join(libs) or "(nenhuma)")
    print("textos de licença usados:", ", ".join(sorted({t for _, ts in achadas for t in ts if t})))
    macos = [(n, v) for n, v in macos if v]
    if macos:
        n, v = max(macos, key=lambda x: x[1])
        print(f"macOS mínimo: {v[0]}.{v[1]} (exigido por {n})")
    erro = 0
    if sem_regra:
        print("ERRO: biblioteca nativa sem regra de licença (atualize THIRD_PARTY_NOTICES.md, "
              "LICENSES/ e packaging/bibliotecas.py):", file=sys.stderr)
        for n in sorted(sem_regra):
            print(f"  {n}", file=sys.stderr)
        erro = 1
    if faltam:
        print("ERRO: texto ausente em LICENSES/: " + ", ".join(sorted(faltam)), file=sys.stderr)
        erro = 1
    return erro


if __name__ == "__main__":
    sys.exit(main())
