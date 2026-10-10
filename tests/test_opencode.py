"""OpenCode na entrada: linha de comando, perfil leitura e leitura dos eventos JSON."""

import json
import queue
import unittest

from hud import config
from hud import opencode as oc
from tests.suporte import fake_executable


def cfg(**kw):
    return oc.OpencodeConfig(executable="/x/opencode", cwd="/", **kw)


class ArgvTest(unittest.TestCase):
    def test_argv(self):
        argv = oc.build_argv(cfg())
        self.assertEqual(argv[:4], ["/x/opencode", "run", "--format", "json"])
        self.assertEqual(argv[argv.index("-m") + 1], oc.DEFAULT_MODEL)
        self.assertNotIn("--session", argv)
        self.assertEqual(oc.build_argv(cfg(), "ses_1")[-2:], ["--session", "ses_1"])
        self.assertNotIn("-m", oc.build_argv(cfg(model="")))
        # O prompt nunca vai na linha de comando: é a entrada padrão.
        self.assertFalse(any("pergunta" in a for a in oc.build_argv(cfg(), "s")))

    def test_read_only_config(self):
        perm = json.loads(oc.read_only_config(("/skills",)))["permission"]
        # ask (e não deny): no `opencode run` é recusado sem tirar a ferramenta da lista.
        for tool in ("edit", "bash", "webfetch", "websearch", "task"):
            self.assertEqual(perm[tool], "ask", tool)
        self.assertEqual(perm["external_directory"], {"/skills/**": "allow", "*": "ask"})
        self.assertEqual(perm["read"]["*"], "allow")
        self.assertEqual(perm["read"]["*.env"], "deny")
        self.assertNotIn("deny", (perm["edit"], perm["bash"]))

    def test_read_only_config_busca_e_mcp(self):
        # AT-060: a regra do grep casa com o padrão buscado, não com o arquivo, e
        # ele procura nos ocultos; fica em ask. "*": "ask" pega o que não tem nome
        # aqui (ferramentas MCP do opencode.json, lsp, skill...).
        raw = oc.read_only_config()
        perm = json.loads(raw)["permission"]
        self.assertEqual(perm["grep"], "ask")
        self.assertEqual(perm["*"], "ask")
        # A última regra que casa vale: "*" tem de vir antes das nomeadas.
        self.assertEqual(next(iter(json.loads(raw)["permission"])), "*")
        for tool in oc.READ_TOOLS:
            self.assertEqual(perm[tool], "allow", tool)
        self.assertNotIn("grep", oc.READ_TOOLS)
        # Nenhuma ferramenta inteira em deny (tiraria da lista; o plano gratuito recusa).
        self.assertNotIn("deny", [v for v in perm.values() if isinstance(v, str)])

    def test_profile_env(self):
        q = queue.Queue()
        a = oc.Opencode(cfg(read_dirs=("/skills",)), q)
        self.assertIn("OPENCODE_CONFIG_CONTENT", a.extra_env())
        a.profile = "completo"
        self.assertEqual(a.extra_env(), {})


class EventsTest(unittest.TestCase):
    def test_events(self):
        q = queue.Queue()
        a = oc.Opencode(cfg(), q)
        a.context = "ctx"
        self.assertTrue(a.prepare("oi").startswith("[Contexto do HUD: ctx]"))
        evs = [
            {"type": "step_start", "sessionID": "ses_9", "part": {"type": "step-start"}},
            {"type": "tool_use", "sessionID": "ses_9", "part": {"tool": "read", "state": {
                "status": "completed", "input": {"filePath": "/v/nota.md"}}}},
            {"type": "tool_use", "sessionID": "ses_9", "part": {"tool": "bash", "state": {
                "status": "error", "input": {"command": "rm -rf x"},
                "error": "The user rejected permission to use this specific tool call."}}},
            {"type": "step_finish", "sessionID": "ses_9", "part": {"reason": "tool-calls", "tokens": {"total": 1000}}},
            {"type": "text", "sessionID": "ses_9", "part": {"type": "text", "text": "oi\x1b]0;x\x07"}},
            {"type": "step_finish", "sessionID": "ses_9", "part": {"reason": "stop", "tokens": {"total": 1500}}},
        ]
        results = [a.handle(e) for e in evs]
        self.assertEqual(a.session.id, "ses_9")
        self.assertIsNone(results[3])  # passo intermediário não encerra
        self.assertIs(results[-1], evs[-1])
        got = [q.get_nowait() for _ in range(3)]
        self.assertEqual(got[0], ("agent_tool", "opencode", "read /v/nota.md"))
        self.assertEqual(got[1][0], "agent_denied")
        self.assertIn("rejected", got[1][2])
        self.assertEqual(got[2], ("agent_text", "opencode", "oi"))
        self.assertEqual(a.finish(results[-1]), (True, "", "conversa 2k tokens"))
        self.assertEqual(oc.build_argv(cfg(), a.session.id)[-1], "ses_9")

    def test_error(self):
        a = oc.Opencode(cfg(), queue.Queue())
        ev = {"type": "error", "error": {"name": "APIError", "data": {"message": "free tier\x1b[2J"}}}
        self.assertIs(a.handle(ev), ev)
        self.assertEqual(a.finish(ev), (False, "free tier", ""))


class ConfigTest(unittest.TestCase):
    def test_model_ids(self):
        common = config._common
        for ok in ("opencode/big-pickle", "ollama/gpt-oss:20b", "gpt-5", ""):
            self.assertEqual(common({"model": ok}, "/")[1], ok)
        for bad in ("-x", "--dangerous", "a b", "a;b", "a$(x)"):
            with self.assertRaises(config.ConfigError, msg=bad):
                common({"model": bad}, "/")

    def test_build_opencode(self):
        o = config.build_opencode({"executable": fake_executable("opencode")})
        self.assertEqual((o.model, o.profile, o.follow_folder), (oc.DEFAULT_MODEL, "leitura", True))
        self.assertEqual(config.build_opencode({"executable": fake_executable("opencode"), "model": ""}).model, "")
        with self.assertRaises(config.ConfigError):
            config.build_opencode({"executable": "/nao/existe/opencode"})


if __name__ == "__main__":
    unittest.main()
