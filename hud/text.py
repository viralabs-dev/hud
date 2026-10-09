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
