"""Agenda local, guardada em Markdown legível fora do Vault.

Formato de cada linha:  - [ ] 2026-10-09 14:00 texto   (hora opcional)
"""

import datetime as dt
import os
import re
import tempfile
import unicodedata
from dataclasses import dataclass
from pathlib import Path

from . import plataforma as plat
from .text import clean_line

LINE = re.compile(r"^- \[([ xX])\] (\d{4}-\d{2}-\d{2})(?: (\d{2}:\d{2}))? (.+)$")

WEEKDAYS = ["segunda", "terca", "quarta", "quinta", "sexta", "sabado", "domingo"]
WEEKDAY_NAMES = ["seg", "ter", "qua", "qui", "sex", "sáb", "dom"]
MAX_TEXT = 300


@dataclass
class Item:
    date: dt.date
    time: str | None
    text: str
    done: bool = False
    source: str = ""  # vazio = agenda local; senão, caminho da nota no Vault

    @property
    def sort_key(self):
        return (self.date, self.time or "99:99", self.text)

    def to_line(self) -> str:
        t = f" {self.time}" if self.time else ""
        return f"- [{'x' if self.done else ' '}] {self.date.isoformat()}{t} {self.text}"


def _fold(s: str) -> str:
    return "".join(c for c in unicodedata.normalize("NFD", s.lower())
                   if not unicodedata.combining(c))


def parse_date(tok: str, today: dt.date) -> dt.date | None:
    t = _fold(tok)
    if t == "hoje":
        return today
    if t == "amanha":
        return today + dt.timedelta(days=1)
    if re.fullmatch(r"\+\d{1,3}", t):
        return today + dt.timedelta(days=int(t[1:]))
    for wd, full in enumerate(WEEKDAYS):
        if len(t) >= 3 and full.startswith(t):
            ahead = (wd - today.weekday()) % 7 or 7
            return today + dt.timedelta(days=ahead)
    try:
        if re.fullmatch(r"\d{4}-\d{2}-\d{2}", t):
            return dt.date.fromisoformat(t)
        m = re.fullmatch(r"(\d{1,2})/(\d{1,2})(?:/(\d{2,4}))?", t)
        if m:
            day, month = int(m.group(1)), int(m.group(2))
            year = int(m.group(3)) if m.group(3) else today.year
            if year < 100:
                year += 2000
            d = dt.date(year, month, day)
            if not m.group(3) and d < today:
                d = d.replace(year=year + 1)
            return d
    except ValueError:
        return None
    return None


def parse_time(tok: str) -> str | None:
    m = re.fullmatch(r"(\d{1,2})(?:[:h](\d{2})?)?", tok.lower())
    if not m or (":" not in tok and "h" not in tok.lower()):
        return None
    h, mi = int(m.group(1)), int(m.group(2) or 0)
    if h > 23 or mi > 59:
        raise ValueError(f"hora inválida: '{tok}'")
    return f"{h:02d}:{mi:02d}"


def parse_entry(arg: str, today: dt.date) -> Item:
    """'amanhã 14h dentista' -> Item. Levanta ValueError com a explicação."""
    toks = arg.split()
    if not toks:
        raise ValueError("uso: /ag <quando> [hora] <texto>")
    date = parse_date(toks[0], today)
    if date is None:
        raise ValueError(f"data não entendida: '{toks[0]}' (hoje, amanhã, +3, sex, 12/10, 2026-10-12)")
    rest = toks[1:]
    time = parse_time(rest[0]) if rest else None
    if time:
        rest = rest[1:]
    text = clean_line(" ".join(rest)).strip()
    if not text:
        raise ValueError("falta o texto do compromisso")
    return Item(date, time, text[:MAX_TEXT])


class Agenda:
    def __init__(self, path: Path):
        self.path = path
        self.items: list[Item] = []
        self.error = ""
        self.load()

    def load(self) -> None:
        self.items = []
        # Windows: st_uid e chmod não dizem nada; a agenda só vale dentro do seu perfil.
        why = plat.private_location_error(self.path)
        if why or (plat.WINDOWS and plat.is_link(self.path)):
            self.error = why or f"{self.path} é link simbólico ou junção"
            return
        try:
            raw = self.path.read_text(encoding="utf-8")
        except FileNotFoundError:
            return
        except OSError as e:
            self.error = str(e)
            return
        for line in raw.splitlines():
            m = LINE.match(line.strip())
            if not m:
                continue
            try:
                d = dt.date.fromisoformat(m.group(2))
            except ValueError:
                continue
            self.items.append(Item(d, m.group(3), clean_line(m.group(4)), m.group(1) != " "))

    def save(self) -> None:
        why = plat.private_location_error(self.path)
        if why:
            raise PermissionError(why)
        self.path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
        body = "# Agenda (HUD)\n\n" + "\n".join(
            i.to_line() for i in sorted(self.items, key=lambda i: i.sort_key)) + "\n"
        fd, tmp = tempfile.mkstemp(dir=self.path.parent, prefix=".agenda-")
        try:
            with os.fdopen(fd, "w", encoding="utf-8") as f:
                f.write(body)
            os.chmod(tmp, 0o600)
            os.replace(tmp, self.path)
        except BaseException:
            os.unlink(tmp)
            raise

    def add(self, item: Item) -> None:
        self.items.append(item)
        self.save()

    def remove(self, item: Item) -> None:
        self.items.remove(item)
        self.save()

    def mark_done(self, item: Item) -> None:
        item.done = True
        self.save()

    def prune(self, today: dt.date, keep_days: int = 30) -> None:
        """Concluídos com mais de `keep_days` saem do arquivo."""
        limit = today - dt.timedelta(days=keep_days)
        before = len(self.items)
        self.items = [i for i in self.items if not (i.done and i.date < limit)]
        if len(self.items) != before:
            self.save()


def upcoming(local: list[Item], vault: list[Item], today: dt.date) -> list[Item]:
    """Pendentes em ordem: atrasados, hoje, próximos."""
    items = [i for i in local + vault if not i.done]
    return sorted(items, key=lambda i: i.sort_key)


def day_label(d: dt.date, today: dt.date) -> str:
    delta = (d - today).days
    stamp = f"{WEEKDAY_NAMES[d.weekday()]} {d:%d/%m}"
    if delta < 0:
        return f"Atrasado · {stamp}"
    if delta == 0:
        return f"Hoje · {stamp}"
    if delta == 1:
        return f"Amanhã · {stamp}"
    return stamp
