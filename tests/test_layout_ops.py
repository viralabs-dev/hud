"""Blocos, painel `agentes`, operações de arraste, `dumps` e geometria do layout."""

import unittest
from pathlib import Path

from hud import layout as L

ROOT = Path(__file__).resolve().parent.parent
CUSTOM = ROOT / "custom"
AUTO = {"comandos": 11, "uso_codex": 4}

BASE = '[[coluna]]\npaineis = ["sistema", "agenda"]\n[[coluna]]\npaineis = ["vault", "saida"]\n'


def lay(extra: str = "", base: str = BASE):
    return L.parse_text(base + extra, "t")


def shape(layout):
    return [(c.largura, c.slots) for c in layout.colunas]


def same(a, b):
    return (shape(a), a.nome, a.descricao, a.autor, a.paineis, a.blocos) == \
        (shape(b), b.nome, b.descricao, b.autor, b.paineis, b.blocos)


def three():
    """Três colunas sem largura, como o modelo `monitor`, mas portátil."""
    return L.parse_text(
        '[[coluna]]\npaineis = [{ id = "sistema", altura = 11 }, "comandos", '
        '{ id = "notas", peso = 1 }]\n'
        '[[coluna]]\npaineis = [{ id = "vault", peso = 40 }, { id = "agenda", peso = 20 }, '
        '{ id = "agentes", peso = 40 }]\n'
        '[[coluna]]\npaineis = [{ id = "saida", peso = 1 }, "uso_claude", "uso_codex"]\n'
        '[[painel]]\nid = "notas"\ntipo = "texto"\narquivo = "notas.md"\n', "tres")


class TestAgentes(unittest.TestCase):
    def test_builtin_any_column(self):
        for base in ('[[coluna]]\npaineis = ["agentes", "saida"]\n',
                     '[[coluna]]\npaineis = ["saida"]\n[[coluna]]\npaineis = ["agentes"]\n'):
            lt = lay(base=base)
            self.assertIn("agentes", lt.ids())
        s = lay(base='[[coluna]]\npaineis = ["agentes", "saida"]\n').colunas[0].slots[0]
        self.assertEqual((s.altura, s.peso), (None, 1.0))
        self.assertNotIn("agentes", L.DEFAULT.ids())

    def test_not_a_custom_id(self):
        with self.assertRaises(L.LayoutError) as cm:
            lay('[[painel]]\nid = "agentes"\ntipo = "texto"\narquivo = "a.md"\n')
        self.assertIn("embutido", str(cm.exception))


class TestBlocos(unittest.TestCase):
    def test_valid(self):
        lt = lay('[[bloco]]\nid = "sistema"\ntitulo = "MÁQUINA"\nmostrar = ["cpu", "mem", "rede"]\n'
                 '[[bloco]]\nid = "agenda"\ndias = 7\n'
                 '[[bloco]]\nid = "pasta"\nmostrar = ["resumo", "atencao"]\n'
                 '[[bloco]]\nid = "saida"\ntitulo = "  "\n')
        self.assertEqual(lt.blocos["sistema"],
                         L.BlocoSpec("sistema", "MÁQUINA", frozenset({"cpu", "mem", "rede"})))
        self.assertEqual(lt.blocos["agenda"], L.BlocoSpec("agenda", dias=7))
        self.assertEqual(lt.blocos["vault"].mostrar, frozenset({"resumo", "atencao"}))
        self.assertEqual(lt.blocos["saida"], L.BlocoSpec("saida"))  # vazio = padrão
        self.assertEqual(lt.avisos, [])
        self.assertEqual(lay().blocos, {})

    def test_agentes_and_all_values(self):
        lt = lay('[[bloco]]\nid = "agentes"\nmostrar = ["processos", "sessoes"]\n'
                 '[[bloco]]\nid = "sistema"\nmostrar = ["cpu", "historico", "mem", "swap", '
                 '"disco", "load", "rede", "sensores"]\n',
                 base='[[coluna]]\npaineis = ["agentes", "sistema", "saida"]\n')
        self.assertEqual(lt.blocos["agentes"].mostrar, frozenset({"processos", "sessoes"}))
        self.assertEqual(len(lt.blocos["sistema"].mostrar), 8)

    def test_unused_bloco_warns(self):
        lt = lay('[[bloco]]\nid = "comandos"\ntitulo = "X"\n')
        self.assertIn("[[bloco]] 'comandos' não está em nenhuma coluna", lt.avisos)

    def test_errors(self):
        cases = [
            ('bloco = 1\n', "[[bloco]] precisa ser uma lista de tabelas"),
            ('bloco = [1]\n', "[[bloco]] precisa ser uma tabela"),
            ('[[bloco]]\ntitulo = "x"\n', "[[bloco]] sem 'id'"),
            ('[[bloco]]\nid = "foo"\n', "[[bloco]] 'foo': não é painel embutido"),
            ('[[bloco]]\nid = "sistema"\n[[bloco]]\nid = "sistema"\n',
             "[[bloco]] 'sistema' declarado duas vezes"),
            ('[[bloco]]\nid = "pasta"\n[[bloco]]\nid = "vault"\n',
             "[[bloco]] 'vault' declarado duas vezes"),
            ('[[bloco]]\nid = "sistema"\ncor = "x"\n', "bloco 'sistema': chave desconhecida: cor"),
            ('[[bloco]]\nid = "comandos"\nmostrar = ["cpu"]\n',
             "bloco 'comandos': chave desconhecida: mostrar"),
            ('[[bloco]]\nid = "sistema"\ndias = 3\n', "bloco 'sistema': chave desconhecida: dias"),
            ('[[bloco]]\nid = "sistema"\ntitulo = 3\n', "bloco 'sistema': 'titulo' precisa ser texto"),
            ('[[bloco]]\nid = "sistema"\ntitulo = "' + "x" * 31 + '"\n',
             "bloco 'sistema': 'titulo' tem até 30 caracteres"),
            ('[[bloco]]\nid = "sistema"\nmostrar = "cpu"\n',
             "bloco 'sistema': 'mostrar' é uma lista não vazia de textos"),
            ('[[bloco]]\nid = "sistema"\nmostrar = []\n',
             "bloco 'sistema': 'mostrar' é uma lista não vazia de textos"),
            ('[[bloco]]\nid = "sistema"\nmostrar = ["cpu", "gpu"]\n',
             "bloco 'sistema': valor desconhecido em 'mostrar': gpu"),
            ('[[bloco]]\nid = "vault"\nmostrar = ["cpu"]\n',
             "bloco 'vault': valor desconhecido em 'mostrar': cpu"),
            ('[[bloco]]\nid = "agentes"\nmostrar = ["resumo"]\n',
             "bloco 'agentes': valor desconhecido em 'mostrar': resumo"),
            ('[[bloco]]\nid = "sistema"\nmostrar = ["cpu", "cpu"]\n',
             "bloco 'sistema': valor repetido em 'mostrar'"),
            ('[[bloco]]\nid = "agenda"\ndias = 0\n', "bloco 'agenda': 'dias' é um inteiro de 1 a 30"),
            ('[[bloco]]\nid = "agenda"\ndias = 31\n', "bloco 'agenda': 'dias' é um inteiro de 1 a 30"),
            ('[[bloco]]\nid = "agenda"\ndias = 2.5\n', "bloco 'agenda': 'dias' é um inteiro de 1 a 30"),
            ('[[bloco]]\nid = "agenda"\ndias = true\n', "bloco 'agenda': 'dias' é um inteiro de 1 a 30"),
        ]
        for extra, msg in cases:
            with self.subTest(extra=extra):
                with self.assertRaises(L.LayoutError) as cm:
                    lay(extra) if not extra.startswith("bloco") else lay(base=extra + BASE)
                self.assertIn(msg, str(cm.exception))

    def test_custom_panel_is_not_a_bloco(self):
        with self.assertRaises(L.LayoutError) as cm:
            lay('[[painel]]\nid = "n"\ntipo = "texto"\narquivo = "n.md"\n[[bloco]]\nid = "n"\n')
        self.assertIn("não é painel embutido", str(cm.exception))


class TestSwap(unittest.TestCase):
    def test_same_column_and_across(self):
        d = L.DEFAULT
        s = L.swap(d, "sistema", "agenda")
        self.assertEqual([x.id for x in s.colunas[0].slots][:3], ["agenda", "comandos", "sistema"])
        self.assertEqual(s.colunas[0].slots[2], L.Slot("sistema", 11))  # a altura viaja
        x = L.swap(d, "sistema", "saida")
        self.assertEqual(x.colunas[0].slots[0], L.Slot("saida", None, 45.0))
        self.assertEqual(x.colunas[1].slots[1], L.Slot("sistema", 11))
        self.assertEqual(x.colunas[0].min_cols, 38)  # limites ficam com a coluna
        self.assertEqual(d.ids()[0], "sistema")  # não muda o original
        self.assertIsNot(x.paineis, d.paineis)

    def test_alias_and_errors(self):
        x = L.swap(L.DEFAULT, "pasta", "sistema")
        self.assertEqual(x.colunas[0].slots[0].id, "vault")
        with self.assertRaises(L.LayoutError):
            L.swap(L.DEFAULT, "sistema", "agentes")

    def test_identity(self):
        self.assertTrue(same(L.swap(L.DEFAULT, "saida", "saida"), L.DEFAULT))


class TestMove(unittest.TestCase):
    def test_within_column(self):
        m = L.move(L.DEFAULT, "uso_codex", 0, 0)
        self.assertEqual(m.colunas[0].slots[0].id, "uso_codex")
        m = L.move(L.DEFAULT, "sistema", 0, 99)  # pos é limitada
        self.assertEqual(m.colunas[0].slots[-1].id, "sistema")
        self.assertTrue(same(L.move(L.DEFAULT, "sistema", 0, 0), L.DEFAULT))

    def test_across_columns(self):
        m = L.move(L.DEFAULT, "agenda", 1, 1)
        self.assertEqual([s.id for s in m.colunas[1].slots], ["vault", "agenda", "saida"])
        self.assertNotIn("agenda", [s.id for s in m.colunas[0].slots])
        self.assertEqual(m.colunas[1].slots[1], L.Slot("agenda", None, 1.0))

    def test_new_column(self):
        m = L.move(L.DEFAULT, "agenda", 2, 0)
        self.assertEqual(len(m.colunas), 3)
        self.assertEqual(m.colunas[2], L.Column(None, (L.Slot("agenda", None, 1.0),)))
        self.assertEqual(m.colunas[0].largura, 36.0)  # 36% cabe: nada encolhe
        L.compute(m, 120, 37, AUTO)  # cabe numa tela de 120

    def test_new_column_shrinks_declared(self):
        lt = lay(base='[[coluna]]\nlargura = 50\npaineis = ["sistema", "agenda"]\n'
                      '[[coluna]]\nlargura = 50\npaineis = ["saida"]\n')
        m = L.move(lt, "agenda", 2, 0)
        self.assertEqual([c.largura for c in m.colunas], [33.3, 33.3, None])
        L.check(m)

    def test_new_column_limit(self):
        four = lay(base="".join(f'[[coluna]]\npaineis = ["{a}", "{b}"]\n' for a, b in
                                (("sistema", "agenda"), ("vault", "saida"), ("comandos", "uso_claude"),
                                 ("uso_codex", "agentes"))))
        with self.assertRaises(L.LayoutError) as cm:
            L.move(four, "agenda", 4, 0)
        self.assertIn("no máximo 4 colunas", str(cm.exception))
        with self.assertRaises(L.LayoutError):
            L.move(four, "agenda", 5, 0)
        with self.assertRaises(L.LayoutError):
            L.move(four, "agenda", -1, 0)

    def test_sole_panel_to_new_column_keeps_count(self):
        four = lay(base='[[coluna]]\npaineis = ["sistema"]\n[[coluna]]\npaineis = ["vault"]\n'
                        '[[coluna]]\npaineis = ["saida"]\n[[coluna]]\npaineis = ["agenda"]\n')
        m = L.move(four, "sistema", 4, 0)
        self.assertEqual(m.ids(), ["vault", "saida", "agenda", "sistema"])

    def test_emptied_column_disappears(self):
        lt = lay(base='[[coluna]]\npaineis = ["sistema"]\n[[coluna]]\npaineis = ["saida"]\n')
        m = L.move(lt, "sistema", 1, 0)
        self.assertEqual(len(m.colunas), 1)
        self.assertEqual(m.ids(), ["sistema", "saida"])

    def test_emptied_column_widths(self):
        # some uma coluna sem largura e não sobra nenhuma: a última fica com o resto,
        # as outras crescem na proporção (20:30 vira 40:60)
        lt = lay(base='[[coluna]]\nlargura = 20\npaineis = ["sistema"]\n'
                      '[[coluna]]\nlargura = 30\npaineis = ["vault"]\n'
                      '[[coluna]]\npaineis = ["saida"]\n')
        m = L.move(lt, "saida", 0, 1)
        self.assertEqual([c.largura for c in m.colunas], [40.0, None])
        # sobra uma só coluna: sem largura
        one = lay(base='[[coluna]]\nlargura = 30\npaineis = ["sistema"]\n'
                       '[[coluna]]\nlargura = 30\npaineis = ["saida"]\n')
        self.assertEqual(L.move(one, "sistema", 1, 0).colunas[0].largura, None)
        # a que sumiu tinha largura e ainda há coluna sem largura: nada muda
        m = L.move(L.move(L.DEFAULT, "agenda", 2, 0), "agenda", 1, 0)
        self.assertEqual([c.largura for c in m.colunas], [36.0, None])

    def test_saida_preserved_and_slot_limit(self):
        for pid in L.DEFAULT.ids():
            for col in range(3):
                m = L.move(L.DEFAULT, pid, col, 0)
                self.assertIn("saida", m.ids())
                self.assertEqual(sorted(m.ids()), sorted(L.DEFAULT.ids()))
        full = lay(base='[[coluna]]\npaineis = ["sistema", "comandos", "agenda", "uso_claude", '
                        '"uso_codex", "vault", "agentes", "x"]\n[[coluna]]\npaineis = ["saida"]\n'
                        '[[painel]]\nid = "x"\ntipo = "texto"\narquivo = "x.md"\n')
        with self.assertRaises(L.LayoutError) as cm:
            L.move(full, "saida", 0, 0)
        self.assertIn("de 1 a 8 painéis", str(cm.exception))
        with self.assertRaises(L.LayoutError):
            L.move(full, "nada", 0, 0)
        with self.assertRaises(L.LayoutError):
            L.move(full, "saida", 0, "1")

    def test_check(self):
        bad = L.Layout("x", colunas=[L.Column(None, (L.Slot("sistema", 11),))])
        with self.assertRaises(L.LayoutError) as cm:
            L.check(bad)
        self.assertIn("'saida' é obrigatório", str(cm.exception))
        bad = L.Layout("x", colunas=[L.Column(None, (L.Slot("saida"), L.Slot("saida")))])
        with self.assertRaises(L.LayoutError):
            L.check(bad)
        bad = L.Layout("x", colunas=[L.Column(95.0, (L.Slot("saida"),))])
        with self.assertRaises(L.LayoutError):
            L.check(bad)
        bad = L.Layout("x", colunas=[L.Column(None, (L.Slot("zzz"), L.Slot("saida")))])
        with self.assertRaises(L.LayoutError):
            L.check(bad)


class TestDumps(unittest.TestCase):
    def roundtrip(self, layout):
        text = L.dumps(layout)
        back = L.parse_text(text, layout.nome)
        self.assertTrue(same(back, layout), text)
        self.assertEqual(L.dumps(back), text)
        return text

    def test_default(self):
        text = self.roundtrip(L.DEFAULT)
        self.assertIn('{ id = "vault", peso = 55 }', text)
        self.assertIn('  "sistema",', text)  # altura padrão: só o id

    def test_models(self):
        for d in sorted(CUSTOM.iterdir()):
            if not (d / "layout.toml").is_file():
                continue
            with self.subTest(modelo=d.name):
                try:
                    lt = L.load(d)
                except L.LayoutError as e:  # monitor: só Linux com `ss`
                    self.skipTest(f"{d.name}: {e}")
                self.roundtrip(lt)

    def test_blocos_paineis_and_escapes(self):
        lt = L.parse_text(
            'nome = "x"\ndescricao = "aspas \\" e \\\\ barra"\nautor = "Ana Ção"\n'
            '[[coluna]]\nlargura = 33.5\npaineis = [{ id = "sistema", altura = 7 }, "log", "agentes"]\n'
            '[[coluna]]\npaineis = [{ id = "saida", peso = 2.5 }, "notas"]\n'
            '[[painel]]\nid = "log"\ntitulo = "LOG \\"x\\""\ntipo = "arquivo"\n'
            'caminho = "~/hud.log"\nlinhas = 50\n'
            '[[painel]]\nid = "notas"\ntipo = "texto"\narquivo = "notas.md"\n'
            '[[bloco]]\nid = "sistema"\ntitulo = "MÁQUINA"\nmostrar = ["rede", "cpu"]\n'
            '[[bloco]]\nid = "agenda"\ndias = 3\n'
            '[[bloco]]\nid = "agentes"\nmostrar = ["sessoes"]\n', "x")
        text = self.roundtrip(lt)
        self.assertIn('mostrar = ["cpu", "rede"]', text)
        self.assertIn("largura = 33.5", text)

    def test_after_ops(self):
        m = L.move(L.swap(L.DEFAULT, "agenda", "vault"), "uso_claude", 2, 0)
        self.roundtrip(m)

    def test_invalid_refused(self):
        with self.assertRaises(L.LayoutError):
            L.dumps(L.Layout("x", colunas=[L.Column(None, (L.Slot("sistema", 11),))]))


class TestGeometry(unittest.TestCase):
    def setUp(self):
        self.r = L.compute(L.DEFAULT, 120, 37, AUTO)
        # sistema (0,0,11,43) comandos (11,0,11,43) agenda (22,0,7,43)
        # uso_claude (29,0,4,43) uso_codex (33,0,4,43) vault (0,43,20,77) saida (20,43,17,77)

    def test_panel_at(self):
        self.assertEqual(L.panel_at(self.r, 0, 0), "sistema")
        self.assertEqual(L.panel_at(self.r, 10, 42), "sistema")
        self.assertEqual(L.panel_at(self.r, 11, 42), "comandos")
        self.assertEqual(L.panel_at(self.r, 0, 43), "vault")
        self.assertEqual(L.panel_at(self.r, 36, 119), "saida")
        self.assertIsNone(L.panel_at(self.r, 37, 0))
        self.assertIsNone(L.panel_at(self.r, 0, 120))
        self.assertIsNone(L.panel_at(self.r, -1, 0))

    def test_on_title(self):
        self.assertEqual(L.on_title(self.r, 0, 5), "sistema")
        self.assertEqual(L.on_title(self.r, 20, 60), "saida")
        self.assertIsNone(L.on_title(self.r, 1, 5))
        self.assertIsNone(L.on_title(self.r, 19, 60))

    def test_neighbor(self):
        n = L.neighbor
        self.assertEqual(n(self.r, "sistema", "down"), "comandos")
        self.assertIsNone(n(self.r, "sistema", "up"))
        self.assertIsNone(n(self.r, "sistema", "left"))
        self.assertEqual(n(self.r, "sistema", "right"), "vault")
        self.assertEqual(n(self.r, "uso_codex", "right"), "saida")
        self.assertEqual(n(self.r, "agenda", "right"), "saida")  # 22..28 sobrepõe só a saída
        self.assertEqual(n(self.r, "comandos", "right"), "vault")  # 11..21: mais sobre o vault
        self.assertEqual(n(self.r, "vault", "left"), "sistema")
        self.assertEqual(n(self.r, "saida", "left"), "agenda")  # 20..36: comandos 2 linhas, agenda 7
        self.assertEqual(n(self.r, "vault", "down"), "saida")
        self.assertIsNone(n(self.r, "saida", "down"))
        self.assertIsNone(n(self.r, "saida", "right"))
        self.assertIsNone(n(self.r, "nada", "up"))
        self.assertEqual(n(self.r, "pasta", "down"), "saida")
        with self.assertRaises(ValueError):
            n(self.r, "saida", "norte")

    def test_drop_target(self):
        d, r = L.DEFAULT, self.r
        # meio de outro painel: troca
        self.assertEqual(L.drop_target(d, r, 28, 60, "sistema"), ("swap", "saida"))
        # meio do próprio painel: nada
        self.assertIsNone(L.drop_target(d, r, 5, 5, "sistema"))
        # título da saída: entra antes dela, na coluna 1
        self.assertEqual(L.drop_target(d, r, 20, 60, "sistema"), ("move", 1, 1))
        # base da saída: depois dela
        self.assertEqual(L.drop_target(d, r, 36, 60, "sistema"), ("move", 1, 2))
        # título do vault: primeiro da coluna 1
        self.assertEqual(L.drop_target(d, r, 0, 60, "agenda"), ("move", 1, 0))
        # mesma coluna: posição conta sem o arrastado
        self.assertEqual(L.drop_target(d, r, 33, 5, "sistema"), ("move", 0, 3))
        # soltar onde já está (base do de cima / título do de baixo): nada
        self.assertIsNone(L.drop_target(d, r, 10, 5, "comandos"))
        self.assertIsNone(L.drop_target(d, r, 11, 5, "sistema"))
        self.assertIsNone(L.drop_target(d, r, 0, 5, "sistema"))
        # fora da tela: nada
        self.assertIsNone(L.drop_target(d, r, 40, 5, "sistema"))
        # borda direita da tela: coluna nova
        self.assertEqual(L.drop_target(d, r, 10, 119, "agenda"), ("move", 2, 0))
        # o resultado aplicado é válido
        for row in range(0, 37, 3):
            for col in range(0, 120, 7):
                t = L.drop_target(d, r, row, col, "agenda")
                if t is None:
                    continue
                new = L.move(d, "agenda", *t[1:]) if t[0] == "move" else L.swap(d, "agenda", t[1])
                self.assertFalse(same(new, d))

    def test_three_columns(self):
        lt = three()
        r = L.compute(lt, 120, 40)
        self.assertEqual([r[p][1] for p in ("sistema", "vault", "saida")], [0, 40, 80])
        self.assertEqual(L.panel_at(r, 0, 40), "vault")
        self.assertEqual(L.on_title(r, 0, 100), "saida")
        self.assertEqual(L.neighbor(r, "sistema", "right"), "vault")
        self.assertEqual(L.neighbor(r, "vault", "right"), "saida")
        self.assertEqual(L.neighbor(r, "agentes", "left"), "notas")
        self.assertEqual(L.neighbor(r, "vault", "down"), "agenda")
        self.assertEqual(L.neighbor(r, "agenda", "down"), "agentes")
        self.assertEqual(L.drop_target(lt, r, 15, 100, "agentes"), ("swap", "saida"))
        self.assertEqual(L.drop_target(lt, r, 10, 119, "agentes"), ("move", 3, 0))
        four = L.move(lt, "agentes", 3, 0)
        self.assertEqual(len(four.colunas), 4)
        r4 = L.compute(four, 120, 40)
        # com 4 colunas a borda direita não cria mais coluna: vira troca/movimento
        self.assertNotEqual(L.drop_target(four, r4, 20, 119, "saida")[0:2], ("move", 4))

    def test_monitor_model(self):
        try:
            lt = L.load(CUSTOM / "monitor")
        except L.LayoutError as e:
            self.skipTest(f"monitor: {e}")
        r = L.compute(lt, 120, 40, AUTO)
        self.assertEqual(L.neighbor(r, "sistema", "right"), "vault")
        self.assertEqual(L.neighbor(r, "syslog", "right"), "saida")
        self.assertEqual(L.panel_at(r, 39, 119), "uso_codex")
        self.assertEqual(L.drop_target(lt, r, 15, 100, "portas"), ("swap", "saida"))


if __name__ == "__main__":
    unittest.main()
