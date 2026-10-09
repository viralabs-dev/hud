"""Mostra o glibc mínimo que o binário onefile exige.

Lê o bootloader e cada biblioteca/extensão embutida no arquivo do PyInstaller e
pega a maior versão GLIBC_x.y referenciada. Uso:
    python -I packaging/glibc_minimo.py build/pyinstaller/dist/hud [-v]
"""

import re
import sys

from PyInstaller.archive.readers import CArchiveReader

VER = re.compile(rb"GLIBC_(\d+)\.(\d+)(?:\.(\d+))?\x00")


def maior(data: bytes) -> tuple[int, ...]:
    vs = [tuple(int(x or 0) for x in m.groups()) for m in VER.finditer(data)]
    return max(vs, default=(0,))


def main() -> int:
    path, verbose = sys.argv[1], "-v" in sys.argv[2:]
    with open(path, "rb") as f:
        boot = f.read()
    arch = CArchiveReader(path)
    alvo = [("(bootloader)", maior(boot[: arch._start_offset] if hasattr(arch, "_start_offset") else boot))]
    for name, entry in arch.toc.items():
        if entry[-1] in ("b", "x", "n"):  # binário, extensão, bootloader-dependência
            data = arch.extract(name)
            if data.startswith(b"\x7fELF"):
                alvo.append((name, maior(data)))
    alvo.sort(key=lambda x: x[1], reverse=True)
    if verbose:
        for name, v in alvo:
            print(f"  {'.'.join(map(str, v)):>8}  {name}")
    print(f"glibc mínimo: {'.'.join(map(str, alvo[0][1]))} (exigido por {alvo[0][0]})")
    libs = sorted(n for n, _ in alvo if n != "(bootloader)" and "lib-dynload/" not in n)
    print("bibliotecas embutidas (fora as extensões do CPython):", ", ".join(libs))
    return 0


if __name__ == "__main__":
    sys.exit(main())
