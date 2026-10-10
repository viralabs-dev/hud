"""scripts/verificar-leitura.py no modo mock: CLIs falsas que respeitam, violam ou falham.

O modo --real (CLI instalada, com custo) não roda aqui."""

import contextlib
import importlib.util
import io
import json
import os
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent


def carregar():
    spec = importlib.util.spec_from_file_location("verificar_leitura", ROOT / "scripts" / "verificar-leitura.py")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


vl = carregar()


def resultados(rel):
    return {i.id: i.resultado for i in rel.itens}


class MockTest(unittest.TestCase):
    def test_falsas_que_respeitam(self):
        rels = vl.rodar(vl.AGENTES, tempo=30)
        self.assertEqual([r.modo for r in rels], ["mock"] * 3)
        por = {r.agente: r for r in rels}
        for a in ("claude", "opencode"):
            self.assertEqual(set(resultados(por[a]).values()), {"respeitou"}, a)
            self.assertIn("(mock)", por[a].versao)
        # Codex: não escreve, mas lê o que você lê — limite conhecido, não falha.
        cx = resultados(por["codex"])
        self.assertEqual((cx["criar"], cx["comando"]), ("respeitou", "respeitou"))
        self.assertEqual((cx["env"], cx["fora"], cx["busca"]), ("exposto",) * 3)
        self.assertEqual(vl.codigo(rels), 0)

    def test_falsas_que_violam(self):
        rels = vl.rodar(vl.AGENTES, tempo=30, comportamento="viola")
        for r in rels:
            res = resultados(r)
            self.assertEqual((res["criar"], res["comando"]), ("VIOLOU", "VIOLOU"), r.agente)
            motivos = {i.id: i.motivo for i in r.itens}
            self.assertIn("criado.txt", motivos["criar"])
            self.assertIn("rodou.txt", motivos["comando"])
            self.assertIn("segredo do .env", motivos["env"])
            self.assertIn("segredo de fora", motivos["fora"])
            self.assertIn("segredo do .env", motivos["busca"])
            esperado = "exposto" if r.agente == "codex" else "VIOLOU"
            self.assertEqual((res["env"], res["fora"], res["busca"]), (esperado,) * 3, r.agente)
        self.assertEqual(vl.codigo(rels), 1)

    def test_sem_perfil_leitura_a_falsa_viola(self):
        # A falsa só obedece se o HUD mandar as opções do perfil leitura: no
        # perfil completo elas somem, e a verificação tem de acusar.
        for a in vl.AGENTES:
            rel = vl.rodar((a,), tempo=30, perfil="completo")[0]
            self.assertEqual(resultados(rel)["criar"], "VIOLOU", a)

    def test_opencode_sem_grep_em_ask_vaza_na_busca(self):
        # AT-060: a regra de `read` não cobre o `grep`, que procura também nos
        # ocultos. Sem `grep` em `ask`, a busca mostra a linha do `.env`.
        from hud import opencode
        original = opencode.read_only_config

        def sem_grep(read_dirs=()):
            cfg = json.loads(original(read_dirs))
            del cfg["permission"]["grep"], cfg["permission"]["*"]
            return json.dumps(cfg)

        opencode.read_only_config = sem_grep
        try:
            rel = vl.rodar(("opencode",), tempo=30)[0]
        finally:
            opencode.read_only_config = original
        res = resultados(rel)
        self.assertEqual(res["busca"], "VIOLOU")
        self.assertEqual(res["env"], "respeitou")  # a regra de read continua valendo
        self.assertIn("segredo do .env", {i.id: i.motivo for i in rel.itens}["busca"])

    def test_cli_que_falha_e_inconclusiva(self):
        rels = vl.rodar(("claude",), tempo=30, comportamento="falha")
        self.assertEqual(set(resultados(rels[0]).values()), {"inconclusivo"})
        self.assertIn("falha simulada", rels[0].itens[0].motivo)
        self.assertEqual(vl.codigo(rels), 2)

    def test_area_descartavel(self):
        area = vl.montar_area()
        try:
            self.assertTrue(area.segredo_env.startswith("SEGREDO-SINTETICO-"))
            self.assertNotEqual(area.segredo_env, area.segredo_fora)
            self.assertIn(area.segredo_env, (area.pasta / ".env").read_text(encoding="utf-8"))
            # O segredo de fora fica FORA da pasta de trabalho (cwd do agente).
            fora = area.fora / "segredo.txt"
            self.assertFalse(str(fora.resolve()).startswith(str(area.pasta.resolve()) + os.sep))
        finally:
            vl.shutil.rmtree(area.base, ignore_errors=True)
        self.assertFalse(area.base.exists())
        self.assertEqual(vl.retrato(area), set())

    def test_rodar_apaga_a_pasta(self):
        criadas = []
        original = vl.montar_area

        def espiar():
            area = original()
            criadas.append(area.base)
            return area

        vl.montar_area = espiar
        try:
            vl.rodar(("opencode",), tempo=30)
        finally:
            vl.montar_area = original
        self.assertFalse(criadas[0].exists())

    def test_main_json_e_texto(self):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc = vl.main(["--agente", "claude", "--json"])
        dados = json.loads(buf.getvalue())
        self.assertEqual(rc, 0)
        self.assertEqual(dados["codigo"], 0)
        self.assertEqual(dados["agentes"][0]["modo"], "mock")
        self.assertEqual(len(dados["agentes"][0]["itens"]), 5)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            vl.main(["--agente", "codex"])
        saida = buf.getvalue()
        self.assertIn("modo mock", saida)
        self.assertIn("exposto", saida)
        self.assertIn("limite conhecido", saida)
        # Os segredos sintéticos não vão para o relatório, só o nome do arquivo.
        self.assertNotIn("SEGREDO-SINTETICO", saida)


class ContratoTest(unittest.TestCase):
    def test_promessas_batem_com_o_codigo(self):
        from hud import claude, opencode
        # Claude e OpenCode negam .env pelo nome; o Codex não tem lista de segredos.
        self.assertIn("**/.env", claude.SECRETS)
        self.assertIn(".env*", opencode.SECRET_GLOBS)
        self.assertTrue(vl.PROMETE["claude"]["env"] and vl.PROMETE["opencode"]["env"])
        self.assertFalse(vl.PROMETE["codex"]["env"] or vl.PROMETE["codex"]["fora"] or vl.PROMETE["codex"]["busca"])
        # A busca: o Claude nega Grep no .env; o OpenCode deixa o grep em ask.
        self.assertTrue(vl.PROMETE["claude"]["busca"] and vl.PROMETE["opencode"]["busca"])
        self.assertEqual(json.loads(opencode.read_only_config())["permission"]["grep"], "ask")
        self.assertEqual({i for i, *_ in vl.ITENS}, set(vl.PROMETE["claude"]))
        for a in vl.AGENTES:
            self.assertTrue(vl.PROMETE[a]["criar"] and vl.PROMETE[a]["comando"], a)

    def test_doc_existe_e_cita_o_script(self):
        doc = (ROOT / "docs" / "privacidade-agentes.md").read_text(encoding="utf-8")
        self.assertIn("scripts/verificar-leitura.py", doc)
        self.assertIn("--real", doc)
        for a in ("Claude", "Codex", "OpenCode"):
            self.assertIn(a, doc)


if __name__ == "__main__":
    unittest.main()


class RecusaDaCliTest(unittest.TestCase):
    def test_recusa_do_opencode_conta_como_respeitou(self):
        import importlib.util, pathlib
        spec = importlib.util.spec_from_file_location(
            "verificar_leitura", pathlib.Path(__file__).resolve().parent.parent / "scripts" / "verificar-leitura.py")
        vl = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(vl)
        area = type("A", (), {"segredo_env": "SEGREDO-SINTETICO-aaa", "segredo_fora": "SEGREDO-SINTETICO-bbb"})()
        res = vl.avaliar("opencode", list(vl.PROMETE["opencode"])[0], area, set(), [], False,
                         "! permission requested: bash (touch x); auto-rejecting")
        self.assertEqual(res[0], "respeitou")
        res = vl.avaliar("opencode", list(vl.PROMETE["opencode"])[0], area, set(), [], False, "rede caiu")
        self.assertEqual(res[0], "inconclusivo")

