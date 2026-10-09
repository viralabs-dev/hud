#!/usr/bin/env python3
"""Renderiza a gravação de docs/telas/ em PNG (um por cena) e num GIF animado.

    python3 scripts/render-screens.py

Lê hud-demo.cast (asciinema v2) e hud-demo.metadata.json, reproduz a saída
num emulador VT (o de tests/vt.py, estendido aqui para guardar fundo, negrito
e dim) e desenha cada célula com uma fonte monoespaçada do sistema (DejaVu
Sans Mono) e a paleta xterm de 256 cores (laranja 208, cinza 248…).

Precisa de Pillow. Se não houver, cria um ambiente virtual TEMPORÁRIO em
$TMPDIR/hud-render-venv, instala o Pillow lá e se executa de novo com ele.
Nada é instalado no Python do sistema nem no repositório; o HUD continua sem
dependências.
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TELAS = ROOT / "docs" / "telas"
NAME = "hud-demo"

try:
    from PIL import Image, ImageDraw, ImageFont
except ImportError:
    if os.environ.get("HUD_RENDER_VENV"):
        sys.exit("render-screens: o Pillow não carregou nem no ambiente temporário")
    venv = Path(tempfile.gettempdir()) / "hud-render-venv"
    py = venv / "bin" / "python"
    if not py.exists():
        print(f"render-screens: criando ambiente temporário com Pillow em {venv}", file=sys.stderr)
        subprocess.check_call([sys.executable, "-m", "venv", str(venv)])
        subprocess.check_call([str(py), "-m", "pip", "install", "-q", "pillow"])
    os.environ["HUD_RENDER_VENV"] = "1"
    os.execv(str(py), [str(py), __file__, *sys.argv[1:]])

sys.path.insert(0, str(ROOT))
from tests.vt import Screen  # noqa: E402

FONT_SIZE = 14
FONT_DIRS = ["/usr/share/fonts", "/usr/local/share/fonts", "/Library/Fonts", "/System/Library/Fonts"]
REGULAR = ["DejaVuSansMono.ttf", "NotoSansMono-Regular.ttf", "LiberationMono-Regular.ttf"]
BOLD = ["DejaVuSansMono-Bold.ttf", "NotoSansMono-Bold.ttf", "LiberationMono-Bold.ttf"]
# Para glifos que a fonte principal não tem (✦, ⚙, ⊘, ↺…).
FALLBACK = ["DejaVuSans.ttf", "NotoSansSymbols2-Regular.ttf", "NotoSansSymbols-Regular.ttf",
            "NotoSansMath-Regular.ttf", "FreeSerif.ttf", "Symbola.ttf"]

BG = (0x1c, 0x1c, 0x1c)
FG = (0xd0, 0xd0, 0xd0)
# 0–15: paleta comum de terminal escuro (Tango/GNOME); 16–255: exatamente a do xterm.
BASE16 = ["2e3436", "cc0000", "4e9a06", "c4a000", "3465a4", "75507b", "06989a", "d3d7cf",
          "555753", "ef2929", "8ae234", "fce94f", "729fcf", "ad7fa8", "34e2e2", "eeeeec"]


def xterm256() -> list[tuple[int, int, int]]:
    pal = [tuple(int(h[i:i + 2], 16) for i in (0, 2, 4)) for h in BASE16]
    steps = [0, 95, 135, 175, 215, 255]
    pal += [(steps[r], steps[g], steps[b]) for r in range(6) for g in range(6) for b in range(6)]
    pal += [(8 + 10 * i,) * 3 for i in range(24)]
    return pal


PALETTE = xterm256()


class AttrScreen(Screen):
    """O emulador dos testes, guardando (frente, fundo, negrito, dim, inverso)
    no lugar só da cor de frente, e a visibilidade do cursor."""

    def resize(self, rows: int, cols: int) -> None:
        super().resize(rows, cols)
        self.attr = {"fg": None, "bg": None, "bold": False, "dim": False, "rev": False}
        self.cursor_on = True

    def _csi(self, params: str, final: str) -> None:
        if params == "?25" and final in "hl":
            self.cursor_on = final == "h"
        super()._csi(params, final)

    def _sgr(self, nums: list[int]) -> None:
        a, i = self.attr, 0
        while i < len(nums):
            n = nums[i]
            if n == 0:
                a.update(fg=None, bg=None, bold=False, dim=False, rev=False)
            elif n == 1:
                a["bold"] = True
            elif n == 2:
                a["dim"] = True
            elif n == 22:
                a["bold"] = a["dim"] = False
            elif n == 7:
                a["rev"] = True
            elif n == 27:
                a["rev"] = False
            elif 30 <= n <= 37:
                a["fg"] = n - 30
            elif n == 39:
                a["fg"] = None
            elif 40 <= n <= 47:
                a["bg"] = n - 40
            elif n == 49:
                a["bg"] = None
            elif 90 <= n <= 97:
                a["fg"] = n - 90 + 8
            elif 100 <= n <= 107:
                a["bg"] = n - 100 + 8
            elif n in (38, 48) and i + 2 < len(nums) and nums[i + 1] == 5:
                a["fg" if n == 38 else "bg"] = nums[i + 2]
                i += 2
            elif n in (38, 48) and i + 4 < len(nums) and nums[i + 1] == 2:
                i += 4
            i += 1
        # A célula guarda o atributo no lugar da cor (tests/vt.py usa self.fg).
        self.fg = (a["fg"], a["bg"], a["bold"], a["dim"], a["rev"])

    def state(self) -> tuple:
        return (tuple(tuple((c[0], c[1]) for c in row) for row in self.cells),
                (self.y, self.x) if self.cursor_on else None)


# ── fontes ───────────────────────────────────────────────────────────────
def find_font(names: list[str]) -> Path | None:
    for d in FONT_DIRS:
        for n in names:
            hits = list(Path(d).rglob(n)) if Path(d).is_dir() else []
            if hits:
                return hits[0]
    return None


class Fonts:
    def __init__(self, size: int):
        reg, bold = find_font(REGULAR), find_font(BOLD)
        if not reg:
            sys.exit("render-screens: nenhuma fonte monoespaçada encontrada (instale DejaVu Sans Mono)")
        self.path = reg
        self.regular = ImageFont.truetype(str(reg), size)
        self.bold = ImageFont.truetype(str(bold or reg), size)
        self.fallbacks = [ImageFont.truetype(str(p), size) for n in FALLBACK if (p := find_font([n]))]
        asc, desc = self.regular.getmetrics()
        self.ascent = asc
        self.cw = round(self.regular.getlength("M"))
        self.ch = asc + desc
        self._cache: dict[tuple[str, bool], ImageFont.FreeTypeFont] = {}
        self._missing = {}

    def _has(self, font, ch: str) -> bool:
        key = id(font)
        if key not in self._missing:
            m = font.getmask("\U0010fffd")
            self._missing[key] = (m.size, bytes(m))
        m = font.getmask(ch)
        return (m.size, bytes(m)) != self._missing[key]

    def pick(self, ch: str, bold: bool):
        k = (ch, bold)
        if k not in self._cache:
            main = self.bold if bold else self.regular
            font = main
            if ch.strip() and not self._has(main, ch):
                font = next((f for f in self.fallbacks if self._has(f, ch)), main)
            self._cache[k] = font
        return self._cache[k]


# ── desenho ──────────────────────────────────────────────────────────────
PAD = 12


def color(idx, default):
    return default if idx is None else PALETTE[idx % 256]


def mix(a, b, t):
    return tuple(round(x * (1 - t) + y * t) for x, y in zip(a, b))


# Linhas de caixa desenhadas à mão, para emendarem sem frestas entre as células.
BOX = {"─": "lr", "│": "ud", "╭": "rd", "╮": "ld", "╰": "ru", "╯": "lu", "┌": "rd", "┐": "ld",
       "└": "ru", "┘": "lu", "├": "udr", "┤": "udl", "┬": "lrd", "┴": "lru", "┼": "lrud"}


def draw_box(d, x, y, cw, chh, ch, fill):
    cx, cy = x + cw // 2, y + chh // 2
    segs = BOX[ch]
    if ch in "╭╮╰╯":  # cantos arredondados: arco de um quarto
        r = min(cw, chh) // 2
        box = {"╭": (cx, cy, cx + 2 * r, cy + 2 * r, 180, 270), "╮": (cx - 2 * r, cy, cx, cy + 2 * r, 270, 360),
               "╰": (cx, cy - 2 * r, cx + 2 * r, cy, 90, 180), "╯": (cx - 2 * r, cy - 2 * r, cx, cy, 0, 90)}[ch]
        d.arc(box[:4], box[4], box[5], fill=fill, width=1)
        if "r" in segs:
            d.line([(cx + r, cy), (x + cw, cy)], fill=fill)
        if "l" in segs:
            d.line([(x, cy), (cx - r, cy)], fill=fill)
        if "d" in segs:
            d.line([(cx, cy + r), (cx, y + chh)], fill=fill)
        if "u" in segs:
            d.line([(cx, y), (cx, cy - r)], fill=fill)
        return
    if "l" in segs:
        d.line([(x, cy), (cx, cy)], fill=fill)
    if "r" in segs:
        d.line([(cx, cy), (x + cw, cy)], fill=fill)
    if "u" in segs:
        d.line([(cx, y), (cx, cy)], fill=fill)
    if "d" in segs:
        d.line([(cx, cy), (cx, y + chh)], fill=fill)


def render(screen: AttrScreen, fonts: Fonts) -> Image.Image:
    cw, chh = fonts.cw, fonts.ch
    img = Image.new("RGB", (screen.cols * cw + 2 * PAD, screen.rows * chh + 2 * PAD), BG)
    d = ImageDraw.Draw(img)
    cur = (screen.y, screen.x) if screen.cursor_on else None
    for r, row in enumerate(screen.cells):
        for c, (ch, attr) in enumerate(row):
            if ch == "":
                continue
            fgi, bgi, bold, dim, rev = attr or (None, None, False, False, False)
            fg, bg = color(fgi, FG), color(bgi, BG)
            if rev or (r, c) == cur:
                fg, bg = bg, fg
            if dim:
                fg = mix(fg, bg, 0.45)
            x, y = PAD + c * cw, PAD + r * chh
            wide = c + 1 < screen.cols and row[c + 1][0] == ""
            if bg != BG:
                d.rectangle([x, y, x + cw * (2 if wide else 1) - 1, y + chh - 1], fill=bg)
            if ch == " ":
                continue
            if ch in BOX:
                draw_box(d, x, y, cw, chh, ch, fg)
                continue
            font = fonts.pick(ch, bold)
            if font is fonts.regular or font is fonts.bold:
                d.text((x, y), ch, font=font, fill=fg)
            else:  # fonte de reserva: centraliza na célula, na mesma linha de base
                w = font.getlength(ch)
                span = cw * (2 if wide else 1)
                d.text((x + (span - w) / 2, y + fonts.ascent), ch, font=font, fill=fg, anchor="ls")
    return img


# ── reprodução ───────────────────────────────────────────────────────────
def load_cast(path: Path):
    with path.open(encoding="utf-8") as f:
        head = json.loads(f.readline())
        events = [json.loads(line) for line in f if line.strip()]
    return head, [(t, data) for t, kind, data in events if kind == "o"]


def replay(head, events):
    """Gerador: (instante, tela) depois de cada evento de saída."""
    s = AttrScreen(head["height"], head["width"])
    for t, data in events:
        s.feed(data.encode("utf-8"))
        yield t, s


def main() -> int:
    meta = json.loads((TELAS / f"{NAME}.metadata.json").read_text(encoding="utf-8"))
    head, events = load_cast(TELAS / f"{NAME}.cast")
    fonts = Fonts(FONT_SIZE)
    print(f"fonte: {fonts.path.name} {FONT_SIZE}px · célula {fonts.cw}×{fonts.ch}")

    # PNG de cada cena: a tela no instante marcado na gravação.
    scenes = sorted(meta["scenes"], key=lambda s: s["time"])
    pngs, i = [], 0
    s = AttrScreen(head["height"], head["width"])
    for sc in scenes:
        while i < len(events) and events[i][0] <= sc["time"]:
            s.feed(events[i][1].encode("utf-8"))
            i += 1
        img = render(s, fonts)
        out = TELAS / f"{sc['name']}.png"
        img.quantize(colors=256, method=Image.Quantize.MEDIANCUT).save(out, optimize=True)
        pngs.append(img)
        print(f"{out.name}: {out.stat().st_size // 1024} KB")

    # Paleta única do GIF, tirada das cenas (evita cintilação entre quadros).
    strip = Image.new("RGB", (pngs[0].width, pngs[0].height * len(pngs)))
    for k, im in enumerate(pngs):
        strip.paste(im, (0, k * im.height))
    pal = strip.quantize(colors=255, method=Image.Quantize.MEDIANCUT)

    # GIF: amostra a tela a cada STEP s; repete o quadro enquanto nada muda;
    # pausas longas viram no máximo MAX_HOLD; cada cena fica parada por SCENE_HOLD.
    STEP, MAX_HOLD, SCENE_HOLD = 0.25, 1.2, 2.5
    scene_times = [sc["time"] for sc in scenes]
    frames, durs, last = [], [], None
    s = AttrScreen(head["height"], head["width"])
    i, t, end = 0, 0.0, events[-1][0] if events else 0.0
    while t <= end + STEP:
        while i < len(events) and events[i][0] <= t:
            s.feed(events[i][1].encode("utf-8"))
            i += 1
        st = s.state()
        is_scene = any(t - STEP < x <= t for x in scene_times)
        if st != last:
            frames.append(render(s, fonts).quantize(palette=pal, dither=Image.Dither.NONE))
            durs.append(STEP)
            last = st
        else:
            durs[-1] = min(durs[-1] + STEP, MAX_HOLD)
        if is_scene:
            durs[-1] = max(durs[-1], SCENE_HOLD)
        t += STEP
    durs[-1] = max(durs[-1], SCENE_HOLD)
    gif = TELAS / f"{NAME}.gif"
    frames[0].save(gif, save_all=True, append_images=frames[1:], duration=[round(x * 1000) for x in durs],
                   loop=0, optimize=False, disposal=1)
    print(f"{gif.name}: {len(frames)} quadros · {sum(durs):.0f}s · {gif.stat().st_size // 1024} KB")
    return 0


if __name__ == "__main__":
    sys.exit(main())
