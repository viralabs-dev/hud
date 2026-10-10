"""Limpeza e medida de texto para o terminal.

Todo texto que vem de fora (saída de comando, nota do Vault, o que se digita)
passa por `clean` antes de chegar à tela: sequências de escape podem mudar o
título do terminal, gravar no clipboard (OSC 52) ou esconder linhas, e
caracteres de controle de direção (bidi) podem disfarçar o que está escrito.
"""

import re
import unicodedata

# CSI, OSC (terminado por BEL ou ST), DCS/SOS/PM/APC e escapes de 2 bytes.
_ESCAPES = re.compile(
    r"\x1b(?:\[[0-?]*[ -/]*[@-~]"
    r"|\][^\x07\x1b]*(?:\x07|\x1b\\)?"
    r"|[PX^_][^\x1b]*(?:\x1b\\)?"
    r"|[ -/]*[0-~])"
)
# C0 (menos \n), DEL e C1, além dos controles bidi e de largura zero.
_CONTROLS = re.compile(
    "[\x00-\x09\x0b-\x1f\x7f-\x9f​-‏‪-‮⁦-⁩﻿]"
)


def clean(s: str) -> str:
    s = _ESCAPES.sub("", s)
    s = s.replace("\r\n", "\n").replace("\t", "    ")
    # Um \r solto sobrescreve a linha (barras de progresso): fica o último trecho.
    s = "\n".join(line.rsplit("\r", 1)[-1] for line in s.split("\n"))
    return _CONTROLS.sub("", s)


def clean_line(s: str) -> str:
    return clean(s).replace("\n", " ")


def char_width(ch: str) -> int:
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


def width(s: str) -> int:
    return sum(char_width(c) for c in s)


def fit(s: str, w: int, ellipsis: bool = True) -> str:
    """Corta `s` para caber em `w` colunas."""
    if w <= 0:
        return ""
    if width(s) <= w:
        return s
    limit = w - 1 if ellipsis else w
    out, n = [], 0
    for ch in s:
        cw = char_width(ch)
        if n + cw > limit:
            break
        out.append(ch)
        n += cw
    return "".join(out) + ("…" if ellipsis else "")


def pad(s: str, w: int) -> str:
    s = fit(s, w)
    return s + " " * (w - width(s))


def wrap(s: str, w: int) -> list[str]:
    """Quebra por largura de coluna, preferindo espaços."""
    if w <= 0:
        return []
    lines: list[str] = []
    for para in s.split("\n"):
        if not para:
            lines.append("")
            continue
        cur, n = "", 0
        for word in re.split(r"(\s+)", para):
            ww = width(word)
            if n + ww <= w:
                cur += word
                n += ww
                continue
            if cur.strip():
                lines.append(cur.rstrip())
            cur, n = "", 0
            if word.isspace():
                continue
            while width(word) > w:
                head = fit(word, w, ellipsis=False)
                lines.append(head)
                word = word[len(head):]
            cur, n = word, width(word)
        lines.append(cur.rstrip())
    return lines


_MD_LINK = re.compile(r"\[\[([^\]|]+)(?:\|([^\]]+))?\]\]")


def strip_markdown(s: str) -> str:
    s = _MD_LINK.sub(lambda m: m.group(2) or m.group(1).rsplit("/", 1)[-1], s)
    s = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", s)
    s = s.replace("**", "").replace("__", "").replace("`", "")
    return re.sub(r"\s+", " ", s).strip()


# Recuo da continuação: num item de lista ("- ", "• ", "1. "), o texto depois do
# marcador; numa linha recuada, o recuo dela mais 2. Assim a quebra fica alinhada.
_LEAD = re.compile(r"^( *)([-*•·] +|\d{1,3}[.)] +)?")


def hanging_indent(line: str) -> int:
    m = _LEAD.match(line)
    lead, bullet = len(m.group(1)), m.group(2) or ""
    # Linha recuada sem marcador continua 2 colunas mais para dentro, para a
    # continuação não parecer um item novo da mesma lista.
    return lead + (len(bullet) if bullet else 2 if lead else 0)


def wrap_hanging(s: str, w: int, indent: int | None = None) -> list[str]:
    """Como `wrap`, mas as linhas de continuação ficam recuadas (`hanging_indent`)."""
    lines = wrap(s, w)
    ind = hanging_indent(s) if indent is None else indent
    if len(lines) <= 1 or ind <= 0 or ind > w // 2:
        return lines
    head = lines[0]
    rest = s[len(head):].lstrip()
    return [head] + [" " * ind + part for part in wrap(rest, w - ind)]



def wrap_runs(runs: list[tuple[str, str]], w: int, hang: int = 0) -> list[list[tuple[str, str]]]:
    """Quebra trechos (estilo, texto) por largura sem perder o estilo de cada um;
    as linhas de continuação começam com `hang` espaços. Pedaços colados de
    estilos diferentes (`código`, por exemplo) são uma palavra só."""
    if w <= 0:
        return []
    if hang > w // 2:
        hang = 0
    # Palavras: listas de pedaços (estilo, texto) entre espaços; o espaço guarda o estilo.
    words: list[tuple[list[tuple[str, str]], tuple[str, str] | None]] = []
    cur_word: list[tuple[str, str]] = []
    for style, text in runs:
        for tok in re.split(r"(\s+)", text):
            if not tok:
                continue
            if tok.isspace():
                words.append((cur_word, (style, tok)))
                cur_word = []
            else:
                cur_word.append((style, tok))
    words.append((cur_word, None))

    lines: list[list[tuple[str, str]]] = []
    line: list[tuple[str, str]] = []
    n = 0

    def push(style: str, text: str) -> None:
        nonlocal n
        if line and line[-1][0] == style:
            line[-1] = (style, line[-1][1] + text)
        else:
            line.append((style, text))
        n += width(text)

    def newline() -> None:
        nonlocal line, n
        lines.append(line)
        line, n = ([("text", " " * hang)] if hang else []), hang

    for pieces, space in words:
        ww = sum(width(t) for _, t in pieces)
        if pieces and n + ww > w and n > (hang if lines else 0):
            newline()
        for style, text in pieces:
            while n + width(text) > w:  # palavra maior que a linha: corta
                head = fit(text, w - n, ellipsis=False) or text[:1]
                push(style, head)
                text = text[len(head):]
                newline()
            if text:
                push(style, text)
        if space and n < w:
            push(space[0], " " if n + width(space[1]) > w else space[1])
    # Sem espaço sobrando no fim das linhas.
    out = []
    for ln in lines + [line]:
        while ln and not ln[-1][1].strip() and len(ln) > 1:
            ln.pop()
        if ln:
            ln[-1] = (ln[-1][0], ln[-1][1].rstrip() or ln[-1][1])
        out.append(ln)
    while len(out) > 1 and not "".join(t for _, t in out[-1]).strip():
        out.pop()
    return out
