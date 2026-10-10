"""As actions dos workflows estão fixadas em commit (AT-058), sem PyYAML.

Parse de texto simples: cada `uses:` precisa apontar para um SHA de 40
hexadecimais com o comentário da versão exata ao lado; cada checkout precisa de
`persist-credentials: false`; cada workflow declara `permissions` no topo; e a
tabela de docs/actions.md lista exatamente os pares action/SHA em uso.
"""

import re
import unittest
from pathlib import Path

RAIZ = Path(__file__).resolve().parent.parent
WORKFLOWS = sorted((RAIZ / ".github" / "workflows").glob("*.yml"))
DOC = RAIZ / "docs" / "actions.md"

USES = re.compile(r"^(?P<ind>\s*)(?:-\s+)?uses:\s*(?P<resto>.*)$")
FIXADA = re.compile(
    r"^(?P<action>[A-Za-z0-9_.-]+/[A-Za-z0-9_./-]+)@(?P<sha>[0-9a-f]{40})"
    r" # (?P<versao>v\d+\.\d+\.\d+)\s*$")


def ler(p: Path) -> list[str]:
    return p.read_text(encoding="utf-8").splitlines()


def usos() -> list[tuple[str, int, str, str]]:
    """(arquivo, linha, indentação, texto depois de `uses:`) de todo workflow."""
    out = []
    for wf in WORKFLOWS:
        for n, linha in enumerate(ler(wf), 1):
            m = USES.match(linha)
            if m:
                out.append((wf.name, n, m.group("ind"), m.group("resto").strip()))
    return out


def passo(linhas: list[str], i: int) -> list[str]:
    """As linhas do passo que começa em linhas[i] (`- uses: ...`)."""
    col = len(linhas[i]) - len(linhas[i].lstrip())
    corpo = [linhas[i]]
    for linha in linhas[i + 1:]:
        if linha.strip() and len(linha) - len(linha.lstrip()) <= col:
            break
        corpo.append(linha)
    return corpo


class WorkflowsTest(unittest.TestCase):
    def test_ha_workflows_e_usos(self):
        self.assertTrue(WORKFLOWS)
        self.assertTrue(usos())

    def test_toda_action_em_sha_com_versao(self):
        for arq, n, _, resto in usos():
            with self.subTest(f"{arq}:{n}"):
                self.assertRegex(resto, FIXADA, f"{arq}:{n} não está fixada em SHA + # vX.Y.Z")

    def test_mesma_action_mesmo_sha(self):
        visto: dict[str, tuple[str, str]] = {}
        for arq, n, _, resto in usos():
            m = FIXADA.match(resto)
            if not m:
                continue
            par = (m.group("sha"), m.group("versao"))
            with self.subTest(f"{arq}:{n}"):
                self.assertEqual(visto.setdefault(m.group("action"), par), par)

    def test_checkout_sem_credencial_persistida(self):
        achou = 0
        for wf in WORKFLOWS:
            linhas = ler(wf)
            for i, linha in enumerate(linhas):
                m = USES.match(linha)
                if not m or not m.group("resto").startswith("actions/checkout@"):
                    continue
                achou += 1
                corpo = "\n".join(passo(linhas, i))
                with self.subTest(f"{wf.name}:{i + 1}"):
                    self.assertRegex(corpo, r"(?m)^\s+persist-credentials:\s*false\s*$")
        self.assertGreater(achou, 0)

    def test_permissions_no_topo(self):
        for wf in WORKFLOWS:
            with self.subTest(wf.name):
                self.assertIn("permissions:", [l.rstrip() for l in ler(wf)])

    def test_estrutura_minima_do_yaml(self):
        for wf in WORKFLOWS:
            texto = wf.read_text(encoding="utf-8")
            with self.subTest(wf.name):
                self.assertNotIn("\t", texto)
                topo = [l.split(":", 1)[0] for l in texto.splitlines() if l and not l.startswith((" ", "#"))]
                for chave in ("name", "on", "permissions", "jobs"):
                    self.assertIn(chave, topo)
                for linha in texto.splitlines():
                    if linha.strip():
                        self.assertEqual((len(linha) - len(linha.lstrip())) % 2, 0, linha)

    def test_doc_lista_os_shas_em_uso(self):
        doc = DOC.read_text(encoding="utf-8")
        tabela = set(re.findall(r"`([A-Za-z0-9_.-]+/[A-Za-z0-9_./-]+)`\s*\|\s*`(v\d+\.\d+\.\d+)`\s*\|\s*`([0-9a-f]{40})`", doc))
        em_uso = set()
        for _, _, _, resto in usos():
            m = FIXADA.match(resto)
            if m:
                em_uso.add((m.group("action"), m.group("versao"), m.group("sha")))
        self.assertEqual(tabela, em_uso)


if __name__ == "__main__":
    unittest.main()
