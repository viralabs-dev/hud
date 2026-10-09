"""Leitura ao vivo do Vault do Obsidian — ou de qualquer pasta local escolhida.
Nunca escreve nada na pasta.

Uma thread varre o Vault a cada N segundos e só relê as notas cuja data de
modificação mudou. A tela pega o último retrato pronto (`snapshot`).
"""

import datetime as dt
import os
import re
import threading
import time
from dataclasses import dataclass, field
from pathlib import Path

from . import plataforma as plat
from .agenda import Item
from .text import clean_line, strip_markdown

MAX_NOTE_BYTES = 2 * 1024 * 1024
# Numa pasta qualquer (um projeto, por exemplo) também valem .txt e .markdown,
# e as pastas de dependência e build ficam de fora da varredura.
NOTE_EXT = (".md", ".markdown", ".txt")
SKIP_DIRS = {"node_modules", "__pycache__", "venv", "target", "dist", "build", "vendor"}
MAX_FILES = 50_000
CARD = re.compile(r"^- \[([ xX])\] (.*)")
DUE = re.compile(r"(?:📅\s*|@\{|\[due::\s*)(\d{4}-\d{2}-\d{2})")
OPEN_TASK = re.compile(r"^\s*- \[ \] (.+)")
TIME_IN_TASK = re.compile(r"@@\{(\d{2}:\d{2})\}|⏰\s*(\d{2}:\d{2})")

# Nomes das colunas variam um pouco entre quadros; normaliza para quatro.
COLUMN_KIND = {
    "a fazer": "todo", "backlog": "todo", "to do": "todo",
    "em andamento": "doing", "fazendo": "doing", "doing": "doing",
    "bloqueado": "blocked", "bloqueados": "blocked", "blocked": "blocked",
    "concluído": "done", "concluido": "done", "feito": "done", "done": "done",
}


@dataclass
class Board:
    name: str
    rel: str
    counts: dict[str, int] = field(default_factory=dict)
    doing: list[str] = field(default_factory=list)
    blocked: list[str] = field(default_factory=list)
    updated: str = ""


@dataclass
class Snapshot:
    ok: bool = False
    error: str = ""
    notes: int = 0
    today: int = 0
    conflicts: list[str] = field(default_factory=list)
    boards: list[Board] = field(default_factory=list)
    recent: list[tuple[str, float]] = field(default_factory=list)
    tasks: list[Item] = field(default_factory=list)
    scanned_at: float = 0.0
    scan_ms: float = 0.0
    obsidian: bool = False
    capped: bool = False


def card_title(raw: str) -> str:
    return clean_line(strip_markdown(raw))


def parse_board(text: str, rel: str) -> Board | None:
    if not text.startswith("---"):
        return None
    end = text.find("\n---", 3)
    front = text[3:end] if end > 0 else ""
    if "kanban-plugin: board" not in front:
        return None
    m = re.search(r"^title:\s*(.+)$", front, re.M)
    name = m.group(1).strip() if m else Path(rel).stem
    name = re.sub(r"^Kanban\s*\((.+)\)$", r"\1", name)
    upd = re.search(r"^atualizado:\s*(\S+)", front, re.M)
    board = Board(clean_line(name), rel, updated=upd.group(1) if upd else "")
    kind = None
    for line in text[end + 4:].splitlines() if end > 0 else []:
        if line.startswith("## "):
            kind = COLUMN_KIND.get(line[3:].strip().lower())
            if kind:
                board.counts.setdefault(kind, 0)
            continue
        if line.startswith("%% kanban:settings"):
            break
        if not kind:
            continue
        m = CARD.match(line)
        if not m:
            continue
        board.counts[kind] += 1
        if kind == "doing":
            board.doing.append(card_title(m.group(2)))
        elif kind == "blocked":
            board.blocked.append(card_title(m.group(2)))
    return board


def parse_tasks(text: str, rel: str) -> list[Item]:
    out = []
    for line in text.splitlines():
        m = OPEN_TASK.match(line)
        if not m:
            continue
        d = DUE.search(m.group(1))
        if not d:
            continue
        try:
            date = dt.date.fromisoformat(d.group(1))
        except ValueError:
            continue
        t = TIME_IN_TASK.search(m.group(1))
        body = re.sub(r"@@?\{[^}]*\}|\[due::[^\]]*\]|[⏰📅⏳🛫✅]\s*\S*", "", m.group(1))
        title = card_title(body)[:200]
        if title:
            out.append(Item(date, (t.group(1) or t.group(2)) if t else None, title, source=rel))
    return out


class VaultWatcher(threading.Thread):
    def __init__(self, root: Path, interval: float = 5.0):
        super().__init__(daemon=True, name="vault")
        self.root = root
        self.interval = interval
        self._cache: dict[str, tuple[float, int, Board | None, list[Item]]] = {}
        self._lock = threading.Lock()
        self._snap = Snapshot()
        self._wake = threading.Event()
        self._stop = threading.Event()
        self.version = 0
        self._next_root: Path | None = None

    @property
    def snapshot(self) -> Snapshot:
        with self._lock:
            return self._snap

    def refresh(self) -> None:
        self._wake.set()

    def set_root(self, root: Path) -> None:
        """Troca a pasta; a thread aplica antes da próxima varredura."""
        with self._lock:
            self._next_root = root
            self._snap = Snapshot()
            self.version += 1
        self._wake.set()

    def stop(self) -> None:
        self._stop.set()
        self._wake.set()

    def run(self) -> None:
        while not self._stop.is_set():
            with self._lock:
                if self._next_root is not None:
                    self.root, self._next_root = self._next_root, None
                    self._cache.clear()
            snap = self.scan()
            if self._next_root is not None:  # trocou no meio: descarta
                continue
            with self._lock:
                self._snap = snap
                self.version += 1
            self._wake.wait(self.interval)
            self._wake.clear()

    def iter_notes(self):
        root = str(self.root)
        n = 0
        for dirpath, dirs, files in os.walk(root, followlinks=False):
            dirs[:] = [d for d in dirs if not d.startswith(".") and d not in SKIP_DIRS]
            if plat.WINDOWS:  # os.walk entra em junções; o HUD não segue link nenhum
                dirs[:] = [d for d in dirs if not plat.is_link(os.path.join(dirpath, d))]
            for f in files:
                if f.lower().endswith(NOTE_EXT) and not f.startswith("."):
                    n += 1
                    if n > MAX_FILES:
                        return
                    yield os.path.join(dirpath, f)

    def scan(self) -> Snapshot:
        t0 = time.monotonic()
        snap = Snapshot()
        if not self.root.is_dir():
            snap.error = f"pasta não encontrada: {self.root}"
            return snap
        snap.obsidian = (self.root / ".obsidian").is_dir()
        midnight = dt.datetime.combine(dt.date.today(), dt.time()).timestamp()
        mtimes: list[tuple[float, str]] = []
        seen = set()
        root = str(self.root)
        try:
            for path in self.iter_notes():
                try:
                    st = os.lstat(path)
                except OSError:
                    continue
                if not os.path.isfile(path) or os.path.islink(path) or plat.is_reparse(st):
                    continue
                rel = os.path.relpath(path, root)
                seen.add(rel)
                snap.notes += 1
                if st.st_mtime >= midnight:
                    snap.today += 1
                if "sync-conflict" in rel:
                    snap.conflicts.append(rel)
                    continue
                mtimes.append((st.st_mtime, rel))
                cached = self._cache.get(rel)
                if not cached or cached[0] != st.st_mtime or cached[1] != st.st_size:
                    board, tasks = None, []
                    if st.st_size <= MAX_NOTE_BYTES:
                        try:
                            with open(path, encoding="utf-8", errors="replace") as f:
                                text = f.read()
                            board, tasks = parse_board(text, rel), parse_tasks(text, rel)
                        except OSError:
                            pass
                    cached = (st.st_mtime, st.st_size, board, tasks)
                    self._cache[rel] = cached
                if cached[2]:
                    snap.boards.append(cached[2])
                snap.tasks.extend(cached[3])
        except OSError as e:
            snap.error = str(e)
            return snap
        for gone in set(self._cache) - seen:
            del self._cache[gone]
        mtimes.sort(reverse=True)
        snap.recent = [(rel, m) for m, rel in mtimes[:12]]
        snap.boards.sort(key=lambda b: (-(b.counts.get("doing", 0)), b.name.lower()))
        snap.conflicts.sort()
        snap.capped = snap.notes >= MAX_FILES
        snap.ok = True
        snap.scanned_at = time.time()
        snap.scan_ms = (time.monotonic() - t0) * 1000
        return snap

    def search(self, term: str, limit: int = 40) -> list[tuple[str, int, str]]:
        """Busca simples, sem regex, sem diferenciar maiúsculas."""
        needle = term.lower()
        root = str(self.root)
        hits: list[tuple[str, int, str]] = []
        for path in self.iter_notes():
            if os.path.islink(path) or (plat.WINDOWS and plat.is_link(path)):
                continue
            rel = os.path.relpath(path, root)
            if needle in rel.lower():
                hits.append((rel, 0, ""))
            try:
                if os.path.getsize(path) > MAX_NOTE_BYTES:
                    continue
                with open(path, encoding="utf-8", errors="replace") as f:
                    for n, line in enumerate(f, 1):
                        if needle in line.lower():
                            hits.append((rel, n, clean_line(line.strip())))
                            break
            except OSError:
                continue
            if len(hits) >= limit:
                break
        return hits
