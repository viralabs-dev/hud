"""Emulador VT mínimo, só com a biblioteca padrão, para testar a tela do HUD.

Entende o que o ncurses manda com TERM=xterm-256color: posicionamento do
cursor, apagar linha/tela, ECH, SGR (só a cor de frente), região de rolagem
com SU/SD, REP, CR/LF/BS/TAB e UTF-8 com largura (W/F ocupam 2 colunas).
Modos privados, charset e o resto são ignorados. Cada célula guarda o
caractere e a cor de frente (índice 256 ou None).

Não é um xterm completo: atributos (negrito, dim, fundo) são descartados, não
há tab stops configuráveis nem margens horizontais.
"""

import codecs
import unicodedata

# DEC Special Graphics (ESC ( 0), caso o ncurses use ACS.
_DEC = dict(zip("jklmnqtuvwx", "┘┐┌└┼─├┤┴┬│"))


def char_width(ch: str) -> int:
    if unicodedata.combining(ch):
        return 0
    return 2 if unicodedata.east_asian_width(ch) in ("W", "F") else 1


class Screen:
    def __init__(self, rows: int, cols: int):
        self.title: str | None = None  # último título pedido por OSC 0/2
        self.osc: list[str] = []
        self._dec = codecs.getincrementaldecoder("utf-8")("replace")
        self._state = "text"
        self._buf = ""
        self.resize(rows, cols)

    # ── estado ───────────────────────────────────────────────────────────
    def resize(self, rows: int, cols: int) -> None:
        """Como um xterm: o conteúdo fica ancorado no canto superior esquerdo."""
        old = getattr(self, "cells", [])
        self.cells = [[(old[r][c] if r < len(old) and c < len(old[r]) else [" ", None])
                       for c in range(cols)] for r in range(rows)]
        self.rows, self.cols = rows, cols
        self.y = self.x = 0
        self.fg: int | None = None
        self.top, self.bottom = 0, rows - 1
        self.wrap_next = False
        self.last = " "
        self.saved = (0, 0)
        self.acs = False

    def _blank(self) -> list:
        return [" ", None]

    # ── leitura ──────────────────────────────────────────────────────────
    def line(self, r: int) -> str:
        return "".join(c[0] for c in self.cells[r])

    def text(self) -> str:
        return "\n".join(self.line(r) for r in range(self.rows))

    def find(self, s: str) -> tuple[int, int] | None:
        """(linha, índice na string da linha) da primeira ocorrência."""
        for r in range(self.rows):
            i = self.line(r).find(s)
            if i >= 0:
                return r, i
        return None

    def fg_at(self, r: int, c: int) -> int | None:
        return self.cells[r][c][1]

    # ── entrada ──────────────────────────────────────────────────────────
    def feed(self, data: bytes) -> None:
        for ch in self._dec.decode(data):
            self._char(ch)

    def _char(self, ch: str) -> None:
        st = self._state
        if st == "text":
            if ch == "\x1b":
                self._state, self._buf = "esc", ""
            elif ch < " " or ch == "\x7f":
                self._control(ch)
            else:
                self._print(ch)
        elif st == "esc":
            if ch == "[":
                self._state, self._buf = "csi", ""
            elif ch == "]":
                self._state, self._buf = "osc", ""
            elif ch in "()*+":
                self._state, self._buf = "charset", ch
            elif ch in "P^_X":
                self._state, self._buf = "str", ""
            elif ch in " #%":
                self._state = "skip1"
            else:
                self._state = "text"
                self._esc(ch)
        elif st == "csi":
            if "@" <= ch <= "~":
                self._state = "text"
                self._csi(self._buf, ch)
            elif ch == "\x1b":
                self._state, self._buf = "esc", ""
            else:
                self._buf += ch
        elif st == "osc":
            if ch == "\x07":
                self._end_osc()
            elif ch == "\x1b":
                self._state = "osc_esc"
            else:
                self._buf += ch
        elif st == "osc_esc":
            self._end_osc()  # ESC \ (ST)
        elif st == "str":
            if ch == "\x1b":
                self._state = "str_esc"
            elif ch == "\x07":
                self._state = "text"
        elif st == "str_esc":
            self._state = "text"
        elif st == "charset":
            if self._buf == "(":
                self.acs = ch == "0"
            self._state = "text"
        elif st == "skip1":
            self._state = "text"

    def _end_osc(self) -> None:
        self._state = "text"
        self.osc.append(self._buf)
        num, _, rest = self._buf.partition(";")
        if num in ("0", "2"):
            self.title = rest

    def _control(self, ch: str) -> None:
        if ch == "\r":
            self.x, self.wrap_next = 0, False
        elif ch in "\n\x0b\x0c":
            self._index()
        elif ch == "\b":
            self.x, self.wrap_next = max(0, self.x - 1), False
        elif ch == "\t":
            self.x = min(self.cols - 1, (self.x // 8 + 1) * 8)
            self.wrap_next = False
        # BEL, SO/SI e o resto: nada.

    def _esc(self, ch: str) -> None:
        if ch == "7":
            self.saved = (self.y, self.x)
        elif ch == "8":
            self.y, self.x = self.saved
            self.wrap_next = False
        elif ch == "D":
            self._index()
        elif ch == "E":
            self.x = 0
            self._index()
        elif ch == "M":
            if self.y == self.top:
                self._scroll_down(1)
            else:
                self.y = max(0, self.y - 1)
        elif ch == "c":
            self.cells = []
            self.resize(self.rows, self.cols)
        # ESC = , ESC > e outros: ignorados.

    # ── impressão ────────────────────────────────────────────────────────
    def _print(self, ch: str) -> None:
        if self.acs:
            ch = _DEC.get(ch, ch)
        w = char_width(ch)
        if w == 0:
            return
        if self.wrap_next or self.x + w > self.cols:
            self.x = 0
            self._index()
            self.wrap_next = False
        row = self.cells[self.y]
        # Não deixar meia letra larga para trás.
        if row[self.x][0] == "" and self.x > 0:
            row[self.x - 1] = self._blank()
        if w == 1 and self.x + 1 < self.cols and row[self.x + 1][0] == "":
            row[self.x + 1] = self._blank()
        row[self.x] = [ch, self.fg]
        if w == 2 and self.x + 1 < self.cols:
            row[self.x + 1] = ["", self.fg]
        self.last = ch
        if self.x + w >= self.cols:
            self.x = self.cols - 1
            self.wrap_next = True
        else:
            self.x += w

    def _index(self) -> None:
        if self.y == self.bottom:
            self._scroll_up(1)
        elif self.y < self.rows - 1:
            self.y += 1

    def _scroll_up(self, n: int) -> None:
        n = min(n, self.bottom - self.top + 1)
        for _ in range(n):
            del self.cells[self.top]
            self.cells.insert(self.bottom, [self._blank() for _ in range(self.cols)])

    def _scroll_down(self, n: int) -> None:
        n = min(n, self.bottom - self.top + 1)
        for _ in range(n):
            del self.cells[self.bottom]
            self.cells.insert(self.top, [self._blank() for _ in range(self.cols)])

    # ── CSI ──────────────────────────────────────────────────────────────
    def _csi(self, params: str, final: str) -> None:
        private = params[:1] in ("?", ">", "<", "=")
        if private:
            return  # modos (?1049h, ?25h, ?1006h…) e pedidos: ignorados
        params = params.rstrip(" !\"$'")
        nums = []
        for p in params.replace(":", ";").split(";") if params else []:
            nums.append(int(p) if p.isdigit() else 0)

        def arg(i: int, default: int = 1) -> int:
            return nums[i] if len(nums) > i and nums[i] else default

        if final != "m":
            self.wrap_next = False
        if final in "Hf":
            self.y = min(self.rows - 1, arg(0) - 1)
            self.x = min(self.cols - 1, arg(1) - 1)
        elif final == "A":
            self.y = max(0, self.y - arg(0))
        elif final == "B":
            self.y = min(self.rows - 1, self.y + arg(0))
        elif final == "C":
            self.x = min(self.cols - 1, self.x + arg(0))
        elif final == "D":
            self.x = max(0, self.x - arg(0))
        elif final == "E":
            self.x, self.y = 0, min(self.rows - 1, self.y + arg(0))
        elif final == "F":
            self.x, self.y = 0, max(0, self.y - arg(0))
        elif final == "G" or final == "`":
            self.x = min(self.cols - 1, arg(0) - 1)
        elif final == "d":
            self.y = min(self.rows - 1, arg(0) - 1)
        elif final == "J":
            self._erase_display(arg(0, 0))
        elif final == "K":
            self._erase_line(arg(0, 0))
        elif final == "X":
            row = self.cells[self.y]
            for c in range(self.x, min(self.cols, self.x + arg(0))):
                row[c] = self._blank()
        elif final == "P":
            row = self.cells[self.y]
            n = min(arg(0), self.cols - self.x)
            del row[self.x:self.x + n]
            row.extend(self._blank() for _ in range(n))
        elif final == "@":
            row = self.cells[self.y]
            n = min(arg(0), self.cols - self.x)
            for _ in range(n):
                row.insert(self.x, self._blank())
            del row[self.cols:]
        elif final in "LM":
            if self.top <= self.y <= self.bottom:
                saved = self.top
                self.top = self.y
                (self._scroll_down if final == "L" else self._scroll_up)(arg(0))
                self.top = saved
                self.x = 0
        elif final == "S":
            self._scroll_up(arg(0))
        elif final == "T":
            self._scroll_down(arg(0))
        elif final == "r":
            top, bottom = arg(0) - 1, arg(1, self.rows) - 1
            if 0 <= top < bottom < self.rows:
                self.top, self.bottom = top, bottom
            else:
                self.top, self.bottom = 0, self.rows - 1
            self.y = self.x = 0
        elif final == "b":
            for _ in range(arg(0)):
                self._print(self.last)
        elif final == "m":
            self._sgr(nums or [0])
        elif final == "s":
            self.saved = (self.y, self.x)
        elif final == "u":
            self.y, self.x = self.saved
        # h/l (modos ANSI), n, c, t, q…: ignorados.

    def _erase_line(self, mode: int) -> None:
        row = self.cells[self.y]
        rng = {0: range(self.x, self.cols), 1: range(0, self.x + 1)}.get(mode, range(self.cols))
        for c in rng:
            row[c] = self._blank()

    def _erase_display(self, mode: int) -> None:
        if mode == 0:
            self._erase_line(0)
            rows = range(self.y + 1, self.rows)
        elif mode == 1:
            self._erase_line(1)
            rows = range(0, self.y)
        else:
            rows = range(self.rows)
        for r in rows:
            self.cells[r] = [self._blank() for _ in range(self.cols)]

    def _sgr(self, nums: list[int]) -> None:
        i = 0
        while i < len(nums):
            n = nums[i]
            if n == 0 or n == 39:
                self.fg = None
            elif 30 <= n <= 37:
                self.fg = n - 30
            elif 90 <= n <= 97:
                self.fg = n - 90 + 8
            elif n in (38, 48):
                if i + 2 < len(nums) and nums[i + 1] == 5:
                    if n == 38:
                        self.fg = nums[i + 2]
                    i += 2
                elif i + 4 < len(nums) and nums[i + 1] == 2:
                    i += 4  # cor RGB: não guardada
            i += 1
