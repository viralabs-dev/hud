"""Markdown das respostas dos agentes, para a SAÍDA.

Só o que os agentes usam de fato: títulos, listas, citações, regras, blocos de
código, tabelas e, dentro da linha, **negrito**, *itálico*, `código` e links.
O resultado são linhas em "trechos" (tipo, texto); a cor de cada tipo é da tela.
O texto já chega limpo de escapes (`text.clean`); aqui nada é executado.

Blocos ```hud-doc e ```hud-custom (propostas de arquivo) viram uma linha só:
o arquivo inteiro na tela só atrapalha a leitura, e a proposta é lida do texto
original, não daqui.
"""

import re
from dataclasses import dataclass, field

# Tipos de trecho: text, bold, italic, code, head, quote, dim, codeline.
Run = tuple[str, str]

_FENCE = re.compile(r"^\s*(`{3,}|~{3,})\s*([\w.+-]*)(.*)$")
_HEAD = re.compile(r"^\s{0,3}(#{1,6})\s+(.*?)\s*#*\s*$")
_RULE = re.compile(r"^\s{0,3}([-*_])(\s*\1){2,}\s*$")
_BULLET = re.compile(r"^(\s*)[-*+]\s+(.*)$")
_NUMBER = re.compile(r"^(\s*)(\d{1,3}[.)])\s+(.*)$")
_QUOTE = re.compile(r"^\s{0,3}>\s?(.*)$")
_TABLE_SEP = re.compile(r"^\s*\|?\s*:?-+:?\s*(\|\s*:?-+:?\s*)*\|?\s*$")
_ATTR = re.compile(r"""(\w+)=("[^"]*"|'[^']*'|\S+)""")
# Ordem importa: código antes de ênfase (o que está entre crases é literal).
_INLINE = re.compile(
    r"(?P<code>`+)(?P<code_t>.+?)(?P=code)"
    r"|\*\*(?P<b1>[^*\s](?:.*?[^*\s])?)\*\*"
    r"|__(?P<b2>[^_\s](?:.*?[^_\s])?)__"
    r"|(?<![\w*])\*(?P<i1>[^*\s](?:[^*]*?[^*\s])?)\*(?![\w*])"
    r"|(?<![\w_])_(?P<i2>[^_\s](?:[^_]*?[^_\s])?)_(?![\w_])"
    r"|\[(?P<lt>[^\]\n]+)\]\((?P<lu>[^)\s]+)\)"
)


@dataclass
class Line:
    runs: list[Run]
    hang: int = 0  # recuo das linhas de continuação


@dataclass
class Table:
    rows: list[list[list[Run]]] = field(default_factory=list)  # linha → célula → trechos
    header: bool = False


def inline(text: str, base: str = "text") -> list[Run]:
    """Trechos de uma linha: **negrito**, *itálico*, `código` e [link](url)."""
    runs: list[Run] = []
    pos = 0
    for m in _INLINE.finditer(text):
        if m.start() > pos:
            runs.append((base, text[pos:m.start()]))
        if m.group("code"):
            runs.append(("code", m.group("code_t").strip()))
        elif m.group("b1") or m.group("b2"):
            runs.append(("bold", m.group("b1") or m.group("b2")))
        elif m.group("i1") or m.group("i2"):
            runs.append(("italic", m.group("i1") or m.group("i2")))
        else:
            runs.append((base, m.group("lt")))
            if m.group("lu") != m.group("lt"):
                runs.append(("dim", f" ({m.group('lu')})"))
        pos = m.end()
    if pos < len(text):
        runs.append((base, text[pos:]))
    return [r for r in runs if r[1]]


def plain(runs: list[Run]) -> str:
    return "".join(t for _, t in runs)


def _cells(line: str) -> list[str]:
    s = line.strip()
    if s.startswith("|"):
        s = s[1:]
    if s.endswith("|") and not s.endswith("\\|"):
        s = s[:-1]
    return [c.strip() for c in re.split(r"(?<!\\)\|", s)]


def render(text: str) -> list[Line | Table]:
    out: list[Line | Table] = []
    lines = text.split("\n")
    i = 0
    while i < len(lines):
        line = lines[i]
        fence = _FENCE.match(line)
        if fence:
            mark, lang, rest = fence.group(1), fence.group(2), fence.group(3)
            body: list[str] = []
            i += 1
            while i < len(lines):
                s = lines[i].strip()
                if s and s[0] == mark[0] and len(s) >= len(mark) and set(s) == {mark[0]}:
                    break
                body.append(lines[i])
                i += 1
            i += 1  # a cerca de fechamento (ou o fim do texto)
            if lang in ("hud-doc", "hud-custom"):
                attrs = {k: v.strip("\"'") for k, v in _ATTR.findall(rest)}
                nome = attrs.get("arquivo", "?")
                if lang == "hud-custom":
                    nome = f"{attrs.get('nome', '?')}/{nome}"
                out.append(Line([("dim", "  ▤ "), ("code", nome),
                                 ("dim", f" · {len(body)} linha(s) · proposta de arquivo")], 4))
                continue
            if lang:
                out.append(Line([("dim", f"  ┌ {lang}")]))
            for b in body or [""]:
                out.append(Line([("dim", "  │ "), ("codeline", b)], 4 + len(b) - len(b.lstrip())))
            continue
        if line.lstrip().startswith("|") and i + 1 < len(lines) and _TABLE_SEP.match(lines[i + 1]):
            table = Table(header=True)
            table.rows.append([inline(c) for c in _cells(line)])
            i += 2
            while i < len(lines) and lines[i].lstrip().startswith("|"):
                table.rows.append([inline(c) for c in _cells(lines[i])])
                i += 1
            out.append(table)
            continue
        i += 1
        if not line.strip():
            out.append(Line([]))
        elif m := _HEAD.match(line):
            out.append(Line([("head", plain(inline(m.group(2))))]))
        elif _RULE.match(line):
            out.append(Line([("dim", "─" * 24)]))
        elif m := _QUOTE.match(line):
            out.append(Line([("dim", "▎ ")] + inline(m.group(1), "quote"), 2))
        elif m := _BULLET.match(line):
            ind = len(m.group(1))
            out.append(Line([("text", " " * ind + "• ")] + inline(m.group(2)), ind + 2))
        elif m := _NUMBER.match(line):
            lead = m.group(1) + m.group(2) + " "
            out.append(Line([("text", lead)] + inline(m.group(3)), len(lead)))
        else:
            ind = len(line) - len(line.lstrip())
            out.append(Line(inline(line), ind + 2 if ind else 0))
    return out
