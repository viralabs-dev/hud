#!/usr/bin/env python3
"""Confere as actions fixadas nos workflows contra as tags oficiais (AT-058).

Para cada `uses: dono/repo@<sha> # vX.Y.Z` de .github/workflows/*.yml, resolve
a tag vX.Y.Z no repositório da action pela API do GitHub (via `gh`, só
leitura), segue tag anotada até o commit e confere que:

- o commit da tag é o SHA fixado;
- a assinatura do commit é verificada pelo GitHub;
- e mostra a versão mais nova da mesma major, para saber se há atualização.

Uso: python3 scripts/conferir-actions.py   (sai com 1 se algo não bater)
"""

import json
import re
import subprocess
import sys
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
FIXADA = re.compile(
    r"uses:\s*(?P<action>[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+)(?P<sub>/[A-Za-z0-9_./-]+)?"
    r"@(?P<sha>[0-9a-f]{40}) # (?P<versao>v\d+\.\d+\.\d+)\s*$")
USES = re.compile(r"^\s*(?:-\s+)?uses:")


def api(caminho: str):
    return json.loads(subprocess.check_output(["gh", "api", caminho], text=True))


def commit_da_tag(repo: str, tag: str) -> str:
    obj = api(f"repos/{repo}/git/ref/tags/{tag}")["object"]
    while obj["type"] == "tag":  # tag anotada: segue até o commit
        obj = api(f"repos/{repo}/git/tags/{obj['sha']}")["object"]
    return obj["sha"]


def mais_nova_da_major(repo: str, versao: str) -> str:
    major = versao.split(".")[0] + "."
    tags = [r["tag_name"] for r in api(f"repos/{repo}/releases?per_page=100")
            if not r["draft"] and not r["prerelease"]]
    vs = [t for t in tags if re.fullmatch(re.escape(major) + r"\d+\.\d+", t)]
    return max(vs, key=lambda t: tuple(int(x) for x in t[1:].split(".")), default=versao)


def main() -> int:
    falhas = 0
    pares: dict[tuple[str, str, str], list[str]] = {}
    for wf in sorted((RAIZ / ".github" / "workflows").glob("*.yml")):
        for n, linha in enumerate(wf.read_text(encoding="utf-8").splitlines(), 1):
            if not USES.match(linha):
                continue
            m = FIXADA.search(linha)
            if not m:
                print(f"FALHA {wf.name}:{n}: não fixada em SHA + # vX.Y.Z: {linha.strip()}")
                falhas += 1
                continue
            pares.setdefault((m["action"], m["versao"], m["sha"]), []).append(f"{wf.name}:{n}")
    for (repo, versao, sha), onde in sorted(pares.items()):
        real = commit_da_tag(repo, versao)
        verif = api(f"repos/{repo}/commits/{sha}")["commit"]["verification"]
        nova = mais_nova_da_major(repo, versao)
        ok = real == sha and verif["verified"]
        falhas += not ok
        print(f"{'ok   ' if ok else 'FALHA'} {repo} {versao} {sha}"
              f" tag->{real[:12]} assinatura={verif['reason']}"
              f"{'' if nova == versao else f' (há {nova})'} [{', '.join(onde)}]")
    return 1 if falhas else 0


if __name__ == "__main__":
    sys.exit(main())
