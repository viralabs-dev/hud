"""Markdown das respostas dos agentes: blocos, trechos e quebra por largura."""

import unittest

from hud import markdown as md
from hud.text import width, wrap_runs


def kinds(line):
    return [k for k, _ in line.runs]


class InlineTest(unittest.TestCase):
    def test_inline(self):
        runs = md.inline("um **forte**, um *leve*, `x_y()` e [doc](https://d.ev/a)")
        self.assertEqual(runs, [("text", "um "), ("bold", "forte"), ("text", ", um "), ("italic", "leve"),
                                ("text", ", "), ("code", "x_y()"), ("text", " e "), ("text", "doc"),
                                ("dim", " (https://d.ev/a)")])

    def test_literal_inside_code_and_words(self):
        self.assertEqual(md.inline("`a **b** c`"), [("code", "a **b** c")])
        # Sublinhado dentro de palavra e asterisco solto não são ênfase.
        self.assertEqual(md.plain(md.inline("snake_case_nome e 2 * 3 * 4")), "snake_case_nome e 2 * 3 * 4")
        self.assertEqual(md.inline("snake_case_nome"), [("text", "snake_case_nome")])


class BlocksTest(unittest.TestCase):
    def test_blocks(self):
        out = md.render("# Título **x**\n\n- item\n  - sub\n3. passo\n> citação\n---\ntexto")
        lines = [b for b in out if isinstance(b, md.Line)]
        self.assertEqual(lines[0].runs, [("head", "Título x")])
        self.assertEqual(lines[1].runs, [])
        self.assertEqual((md.plain(lines[2].runs), lines[2].hang), ("• item", 2))
        self.assertEqual((md.plain(lines[3].runs), lines[3].hang), ("  • sub", 4))
        self.assertEqual((md.plain(lines[4].runs), lines[4].hang), ("3. passo", 3))
        self.assertEqual(lines[5].runs[0], ("dim", "▎ "))
        self.assertTrue(md.plain(lines[6].runs).startswith("───"))

    def test_code_fence_keeps_content_literal(self):
        out = md.render("```python\nx = **1**\n  y\n```\ndepois")
        self.assertEqual(out[0].runs, [("dim", "  ┌ python")])
        self.assertEqual(out[1].runs, [("dim", "  │ "), ("codeline", "x = **1**")])
        self.assertEqual(out[2].hang, 6)  # recuo do próprio código
        self.assertEqual(md.plain(out[3].runs), "depois")

    def test_proposals_collapse(self):
        out = md.render('````hud-doc arquivo="A/B (X).md"\n# t\n```\nc\n```\nfim\n````\n'
                        "```hud-custom nome=foco arquivo=layout.toml\na\nb\n```")
        self.assertEqual(len(out), 2)
        self.assertIn("A/B (X).md", md.plain(out[0].runs))
        self.assertIn("5 linha(s)", md.plain(out[0].runs))
        self.assertIn("foco/layout.toml", md.plain(out[1].runs))

    def test_table(self):
        out = md.render("| a | b |\n|---|:-:|\n| 1 | **2** |\nfim")
        self.assertIsInstance(out[0], md.Table)
        self.assertTrue(out[0].header)
        self.assertEqual([[md.plain(c) for c in r] for r in out[0].rows], [["a", "b"], ["1", "2"]])
        self.assertEqual(md.plain(out[1].runs), "fim")
        # Sem a linha de separação não é tabela.
        self.assertIsInstance(md.render("| só | texto |")[0], md.Line)


class WrapRunsTest(unittest.TestCase):
    def test_keeps_styles_and_width(self):
        runs = [("text", "O mais antigo é o "), ("code", "AT-1"), ("text", ", parado desde "),
                ("italic", "25/09"), ("text", ". " + "palavra " * 6)]
        for w in (12, 20, 31, 50):
            lines = wrap_runs(runs, w, 2)
            for ln in lines:
                self.assertLessEqual(sum(width(t) for _, t in ln), w)
            joined = " ".join("".join(t for _, t in ln).strip() for ln in lines)
            self.assertIn("AT-1,", joined)  # vírgula colada ao código não se separa
            self.assertTrue(all("".join(t for _, t in ln).startswith("  ") for ln in lines[1:]))
            styles = {s for ln in lines for s, _ in ln}
            self.assertTrue({"code", "italic"} <= styles)

    def test_long_word_is_cut(self):
        lines = wrap_runs([("code", "x" * 25)], 10)
        self.assertEqual([len(ln[0][1]) for ln in lines], [10, 10, 5])


if __name__ == "__main__":
    unittest.main()
