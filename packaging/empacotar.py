"""Monta o pacote de release de forma reproduzível e atualiza dist/checksums.txt.

    python -I packaging/empacotar.py <binário> <dist/hud_<so>_<arch>.tar.gz|.zip> <epoch>

O pacote leva o binário (hud ou hud.exe), LICENSE, THIRD_PARTY_NOTICES.md e
LICENSES/*.txt, em ordem fixa, com dono 0, datas fixas (epoch do último commit)
e permissões 755/644. .tar.gz para Linux e macOS, .zip para o Windows. Só usa
a biblioteca padrão, então roda igual no Linux, no macOS e no Windows (o
`tar` do macOS e o `sha256sum` não são os mesmos em toda parte).

Depois grava dist/checksums.txt com o SHA-256 de todos os dist/hud_* (formato
do sha256sum: "<hash>  <arquivo>").
"""

import gzip
import hashlib
import io
import os
import sys
import tarfile
import time
import zipfile

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), ".."))


def membros(binario: str):
    """(nome no pacote, caminho local, modo) em ordem fixa."""
    nome_bin = "hud.exe" if binario.lower().endswith(".exe") else "hud"
    itens = [(nome_bin, binario, 0o755),
             ("LICENSE", os.path.join(ROOT, "LICENSE"), 0o644),
             ("THIRD_PARTY_NOTICES.md", os.path.join(ROOT, "THIRD_PARTY_NOTICES.md"), 0o644)]
    lic = os.path.join(ROOT, "LICENSES")
    for f in sorted(os.listdir(lic)):
        if f.endswith(".txt"):
            itens.append((f"LICENSES/{f}", os.path.join(lic, f), 0o644))
    return sorted(itens, key=lambda x: x[0])


def tar_gz(destino: str, itens, epoch: int) -> None:
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w", format=tarfile.GNU_FORMAT) as tar:
        dirs_feitos = set()
        for nome, caminho, modo in itens:
            if "/" in nome:
                d = nome.rsplit("/", 1)[0]
                if d not in dirs_feitos:
                    dirs_feitos.add(d)
                    ti = tarfile.TarInfo(d)
                    ti.type, ti.mode, ti.mtime = tarfile.DIRTYPE, 0o755, epoch
                    ti.uid = ti.gid = 0
                    ti.uname = ti.gname = ""
                    tar.addfile(ti)
            with open(caminho, "rb") as f:
                data = f.read()
            ti = tarfile.TarInfo(nome)
            ti.size, ti.mode, ti.mtime = len(data), modo, epoch
            ti.uid = ti.gid = 0
            ti.uname = ti.gname = ""
            tar.addfile(ti, io.BytesIO(data))
    with open(destino, "wb") as raw:
        with gzip.GzipFile(filename="", mode="wb", fileobj=raw, compresslevel=9, mtime=0) as gz:
            gz.write(buf.getvalue())


def zip_(destino: str, itens, epoch: int) -> None:
    data_hora = time.gmtime(max(epoch, 315532800))[:6]  # o zip não guarda datas antes de 1980
    with zipfile.ZipFile(destino, "w", compression=zipfile.ZIP_DEFLATED, compresslevel=9) as z:
        for nome, caminho, modo in itens:
            zi = zipfile.ZipInfo(nome, date_time=data_hora)
            zi.compress_type = zipfile.ZIP_DEFLATED
            zi.create_system = 3  # Unix, para o modo abaixo valer em quem extrai no Linux
            zi.external_attr = (0o100000 | modo) << 16
            with open(caminho, "rb") as f:
                z.writestr(zi, f.read())


def checksums(dist: str) -> str:
    linhas = []
    for f in sorted(os.listdir(dist)):
        if f.startswith("hud_") and (f.endswith(".tar.gz") or f.endswith(".zip")):
            h = hashlib.sha256()
            with open(os.path.join(dist, f), "rb") as fh:
                for bloco in iter(lambda: fh.read(1 << 20), b""):
                    h.update(bloco)
            linhas.append(f"{h.hexdigest()}  {f}\n")
    with open(os.path.join(dist, "checksums.txt"), "w", encoding="ascii", newline="\n") as out:
        out.writelines(linhas)
    return "".join(linhas)


def main() -> int:
    if len(sys.argv) != 4:
        print(__doc__, file=sys.stderr)
        return 2
    binario, destino, epoch = sys.argv[1], sys.argv[2], int(sys.argv[3])
    itens = membros(binario)
    os.makedirs(os.path.dirname(os.path.abspath(destino)), exist_ok=True)
    if destino.endswith(".tar.gz"):
        tar_gz(destino, itens, epoch)
    elif destino.endswith(".zip"):
        zip_(destino, itens, epoch)
    else:
        print(f"Extensão desconhecida: {destino}", file=sys.stderr)
        return 2
    print(f"Gerado: {destino} ({os.path.getsize(destino) // 1024} KiB; {len(itens)} arquivos)")
    sys.stdout.write(checksums(os.path.dirname(os.path.abspath(destino))))
    return 0


if __name__ == "__main__":
    sys.exit(main())
