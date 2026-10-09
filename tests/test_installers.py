"""Instaladores, pacotes e workflows de release (Linux, macOS e Windows).

Só biblioteca padrão e leitura de arquivos: roda nas três plataformas do CI. A
execução de verdade dos instaladores fica em scripts/test-installer.sh e
scripts/test-installer.ps1.
"""

import ast
import importlib.util
import io
import os
import re
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def ler(rel: str) -> str:
    return (ROOT / rel).read_text(encoding="utf-8")


def carregar(rel: str, nome: str):
    spec = importlib.util.spec_from_file_location(nome, ROOT / rel)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def requisitos(rel: str) -> dict:
    """nome -> (versão, [hashes]) de um requirements com --require-hashes."""
    texto = ler(rel).replace("\\\n", " ")
    out = {}
    for linha in texto.splitlines():
        linha = linha.split("#", 1)[0].strip()
        if not linha:
            continue
        m = re.match(r"^([A-Za-z0-9_.-]+)==(\S+)\s+(.*)$", linha)
        assert m, f"{rel}: linha sem versão fixa: {linha}"
        hashes = re.findall(r"--hash=sha256:([0-9a-f]{64})", m.group(3))
        out[m.group(1).lower()] = (m.group(2), hashes)
    return out


class InstaladorPowerShellTest(unittest.TestCase):
    def setUp(self):
        self.raw = (ROOT / "install.ps1").read_bytes()
        self.ps = self.raw.decode("ascii")
        # Código sem comentários de linha (o cabeçalho documenta o que não se faz).
        self.codigo = "\n".join(l for l in self.ps.splitlines() if not l.lstrip().startswith("#"))

    def test_so_ascii_sem_bom(self):
        # 5.1 lê .ps1 sem BOM como ANSI; um BOM quebra o `irm | iex`.
        self.assertFalse(self.raw.startswith(b"\xef\xbb\xbf"))
        self.assertTrue(all(b < 128 for b in self.raw))
        for rel in ("scripts/test-installer.ps1", "packaging/smoke.ps1"):
            self.assertTrue(all(b < 128 for b in (ROOT / rel).read_bytes()), rel)

    def test_requisitos_de_seguranca(self):
        c = self.codigo
        self.assertIn("[Net.ServicePointManager]::SecurityProtocol", c)
        self.assertIn("Tls12", c)
        self.assertIn("-UseBasicParsing", c)
        self.assertIn("Get-FileHash", c)
        self.assertIn("System.IO.Compression", c)
        self.assertIn("ReparsePoint", c)
        self.assertIn("WindowsBuiltInRole]::Administrator", c)
        self.assertIn("HKCU", self.ps)  # documentado: PATH do usuário
        self.assertIn("CurrentUser.OpenSubKey('Environment'", c)
        self.assertNotIn("LocalMachine", c)  # nunca o PATH da máquina
        self.assertNotIn("'Machine'", c)
        self.assertNotRegex(c, r"(?i)Set-ExecutionPolicy")
        self.assertNotRegex(c, r"(?i)Read-Host|ReadKey|\[Console\]::Read")
        self.assertNotRegex(c, r"(?i)Expand-Archive|ExtractToDirectory")  # extrai membro a membro
        self.assertIn("-ExecutionPolicy Bypass -File", self.ps)
        self.assertIn("LOCALAPPDATA", c)
        self.assertIn("'Programs'", c)

    def test_mesmas_validacoes_do_install_sh(self):
        sh = ler("install.sh")
        tag_sh = re.search(r'"\$version" =~ (\^v\S+\$) ]]', sh).group(1)
        repo_sh = re.search(r'\[\[ "\$repo" =~ (\^\S+\$) ]]', sh).group(1)
        tag_ps = re.search(r"-cnotmatch '\\A(v\S+)\\z'", self.ps).group(1)
        repo_ps = re.search(r"\$repo -cnotmatch '\\A(\S+)\\z'", self.ps).group(1)
        self.assertEqual(tag_sh.strip("^$"), tag_ps)
        self.assertEqual(repo_sh.strip("^$"), repo_ps)

    def test_base_de_teste_exige_https_fora_do_modo_de_teste(self):
        self.assertIn("HUD_INSTALLER_TEST", self.codigo)
        self.assertRegex(self.codigo, r"\$teste -and \$local")
        self.assertIn("'https://github.com'", self.codigo)

    def test_iex_nao_fecha_o_terminal(self):
        # `exit` só no modo arquivo ($PSCommandPath vazio no irm | iex).
        self.assertIn("if ($ModoArquivo) { exit 1 }", self.codigo)
        self.assertIn("([bool]$PSCommandPath)", self.codigo)
        self.assertEqual(len(re.findall(r"\bexit\b", self.codigo)), 1)


class InstaladorShellTest(unittest.TestCase):
    def test_macos(self):
        sh = ler("install.sh")
        self.assertIn("Darwin) platform=darwin", sh)
        self.assertIn("shasum -a 256", sh)
        self.assertIn("xattr -d com.apple.quarantine", sh)
        self.assertIn("notarizado", sh)
        self.assertIn('asset="hud_${platform}_${arch}.tar.gz"', sh)

    def test_scripts_lf(self):
        for rel in ("install.sh", "install.ps1", "scripts/release.sh", "scripts/test-installer.sh",
                    "packaging/smoke.sh", "packaging/smoke.ps1", "scripts/test-installer.ps1"):
            self.assertNotIn(b"\r", (ROOT / rel).read_bytes(), rel)
        self.assertIn("* text=auto eol=lf", ler(".gitattributes"))


class RequirementsTest(unittest.TestCase):
    ARQS = {
        "linux": "packaging/requirements-build.txt",
        "macos": "packaging/requirements-build-macos.txt",
        "windows": "packaging/requirements-build-windows.txt",
    }

    def test_hashes_e_versoes(self):
        reqs = {k: requisitos(v) for k, v in self.ARQS.items()}
        for plat, r in reqs.items():
            for nome, (versao, hashes) in r.items():
                self.assertTrue(hashes, f"{plat}: {nome} sem hash")
        # As dependências comuns têm a mesma versão nas três plataformas.
        for nome in ("pyinstaller", "pyinstaller-hooks-contrib", "altgraph", "packaging", "setuptools"):
            versoes = {plat: r[nome][0] for plat, r in reqs.items()}
            self.assertEqual(len(set(versoes.values())), 1, f"{nome}: {versoes}")
        self.assertIn("macholib", reqs["macos"])
        for nome in ("pefile", "pywin32-ctypes", "windows-curses"):
            self.assertIn(nome, reqs["windows"])
        self.assertNotIn("windows-curses", reqs["linux"])
        teste = requisitos("packaging/requirements-test-windows.txt")
        self.assertEqual(teste["windows-curses"], reqs["windows"]["windows-curses"])

    def test_ci_usa_require_hashes(self):
        for wf in (".github/workflows/ci.yml", ".github/workflows/release.yml"):
            texto = ler(wf)
            for linha in texto.splitlines():
                if "pip install" in linha:
                    self.assertIn("--require-hashes", linha, f"{wf}: {linha.strip()}")


class LicencasTest(unittest.TestCase):
    def regras(self):
        arvore = ast.parse(ler("packaging/bibliotecas.py"))
        for no in arvore.body:
            if isinstance(no, ast.Assign) and getattr(no.targets[0], "id", "") == "REGRAS":
                return ast.literal_eval(no.value)
        self.fail("REGRAS não encontrada")

    def test_textos_existem_e_estao_nos_avisos(self):
        avisos = ler("THIRD_PARTY_NOTICES.md")
        textos = {t for _, ts in self.regras() for t in ts if t}
        for t in textos:
            self.assertTrue((ROOT / "LICENSES" / t).is_file(), t)
        for f in (ROOT / "LICENSES").glob("*.txt"):
            self.assertIn(f"LICENSES/{f.name}", avisos, f.name)
        for termo in ("windows-curses", "PDCurses", "domínio público", "hud_windows_amd64",
                      "hud_darwin_", "notarizado", "SmartScreen"):
            self.assertIn(termo, avisos)

    def test_classificacao(self):
        regras = self.regras()

        def classe(nome):
            for padrao, textos in regras:
                if re.fullmatch(padrao, nome.lower()):
                    return textos
            return None

        self.assertEqual(classe("_curses.cp313-win_amd64.pyd"), ("windows-curses.txt", "pdcurses.txt"))
        self.assertEqual(classe("_curses.pyd"), ("windows-curses.txt", "pdcurses.txt"))
        self.assertEqual(classe("libffi-8.dll"), ("libffi.txt",))
        self.assertEqual(classe("libncursesw.5.dylib"), ("ncurses.txt",))
        self.assertEqual(classe("libncursesw.so.6"), ("ncurses.txt",))
        self.assertEqual(classe("python313.dll")[0], "python.txt")
        self.assertIsNone(classe("libssl.3.dylib"))   # OpenSSL fica fora; se aparecer, a release falha
        self.assertIsNone(classe("libcrypto-3.dll"))

    def test_spec_por_plataforma(self):
        spec = ler("packaging/hud.spec")
        self.assertIn('PLATAFORMA == "linux"', spec)
        self.assertRegex(spec, r'HIDDEN \+= \["ctypes", "ctypes.wintypes", "_curses_panel"')
        self.assertRegex(spec, r'HIDDEN \+= \["ctypes", "ctypes.util"\]')


class EmpacotarTest(unittest.TestCase):
    def setUp(self):
        self.emp = carregar("packaging/empacotar.py", "empacotar_teste")
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)

    def tearDown(self):
        self.tmp.cleanup()

    def montar(self, nome_bin, destino):
        b = self.dir / nome_bin
        b.write_bytes(b"#!/bin/sh\necho hud 0.0.0\n")
        itens = self.emp.membros(str(b))
        if destino.endswith(".zip"):
            self.emp.zip_(destino, itens, 1700000000)
        else:
            self.emp.tar_gz(destino, itens, 1700000000)
        return itens

    def test_zip_windows(self):
        d1, d2 = str(self.dir / "a" / "hud_windows_amd64.zip"), str(self.dir / "b" / "hud_windows_amd64.zip")
        os.makedirs(os.path.dirname(d1)); os.makedirs(os.path.dirname(d2))
        self.montar("hud.exe", d1)
        self.montar("hud.exe", d2)
        self.assertEqual(Path(d1).read_bytes(), Path(d2).read_bytes())  # reproduzível
        with zipfile.ZipFile(d1) as z:
            nomes = z.namelist()
        self.assertIn("hud.exe", nomes)
        self.assertIn("LICENSE", nomes)
        self.assertIn("THIRD_PARTY_NOTICES.md", nomes)
        self.assertIn("LICENSES/windows-curses.txt", nomes)
        self.assertIn("LICENSES/pdcurses.txt", nomes)
        for n in nomes:  # o que o install.ps1 aceita
            self.assertRegex(n, r"\A(hud\.exe|LICENSE|THIRD_PARTY_NOTICES\.md|LICENSES/[A-Za-z0-9._-]+\.txt)\Z")
        sums = self.emp.checksums(os.path.dirname(d1))
        self.assertRegex(sums, r"\A[0-9a-f]{64}  hud_windows_amd64\.zip\n\Z")

    def test_tar_unix(self):
        d = str(self.dir / "hud_darwin_arm64.tar.gz")
        self.montar("hud", d)
        with tarfile.open(d) as t:
            membros = {m.name: m for m in t.getmembers()}
        self.assertEqual(membros["hud"].mode, 0o755)
        self.assertEqual(membros["LICENSE"].mode, 0o644)
        self.assertTrue(membros["LICENSES"].isdir())
        self.assertTrue(all(m.uid == 0 and m.mtime == 1700000000 for m in membros.values()))
        self.assertEqual(Path(d).read_bytes()[4:8], b"\0\0\0\0")  # gzip sem data


class WorkflowsTest(unittest.TestCase):
    def test_release_cinco_pacotes_e_checksum_unico(self):
        rel = ler(".github/workflows/release.yml")
        pacotes = ["hud_linux_amd64.tar.gz", "hud_linux_arm64.tar.gz", "hud_darwin_arm64.tar.gz",
                   "hud_darwin_amd64.tar.gz", "hud_windows_amd64.zip"]
        bloco = rel.split("checksums.txt único", 1)[1].split("gh release create", 1)[0]
        for p in pacotes:
            self.assertIn(f"asset: {p}", rel)
            self.assertIn(p, bloco)
        self.assertIn("GH_TOKEN: ${{ github.token }}", rel)
        self.assertNotRegex(rel, r"secrets\.")

    def test_checkout_sem_credenciais_e_permissoes_minimas(self):
        for wf in (".github/workflows/ci.yml", ".github/workflows/release.yml"):
            texto = ler(wf)
            self.assertEqual(texto.count("actions/checkout@"), texto.count("persist-credentials: false"), wf)
            self.assertRegex(texto, r"(?m)^permissions:\n  contents: read$", wf)
            self.assertNotIn("contents: write", texto.split("  release:", 1)[0], wf)

    def test_ci_tres_sistemas(self):
        ci = ler(".github/workflows/ci.yml")
        for runner in ("ubuntu-latest", "macos-latest", "windows-latest", "ubuntu-22.04", "macos-14"):
            self.assertIn(runner, ci)
        self.assertIn("requirements-test-windows.txt", ci)
        self.assertIn("test-installer.ps1 -Registry", ci)
        self.assertIn("-Shell powershell.exe", ci)
        self.assertIn("bash scripts/test-installer.sh", ci)


class ReadmeTest(unittest.TestCase):
    def test_instalar(self):
        readme = ler("README.md")
        secao = readme.split("## Instalar", 1)[1].split("\n## ", 1)[0]
        for termo in ("install.ps1 | iex", "-ExecutionPolicy Bypass -File", "%LOCALAPPDATA%\\Programs\\hud",
                      "PATH **do usuário**", "-Uninstall", "Gatekeeper", "xattr -d com.apple.quarantine",
                      "hud_windows_amd64.zip", "hud_darwin_<arch>.tar.gz"):
            self.assertIn(termo, secao)


if __name__ == "__main__":
    unittest.main()
