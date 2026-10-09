# Avisos de terceiros

O código-fonte do HUD não tem dependências: só a biblioteca padrão do Python e o
módulo `curses`. Este arquivo vale para o **binário autocontido** (`hud` dos
pacotes `hud_linux_<arch>.tar.gz` das releases), que embute os componentes
abaixo. Os textos completos ficam em `LICENSES/`, dentro do pacote e, depois de
instalar, em `~/.local/bin/hud-licenses/`.

| Componente | Para que serve no binário | Licença | Texto |
|---|---|---|---|
| CPython 3.13 (interpretador `libpython3.13.so`, biblioteca padrão e extensões de `lib-dynload`) | roda o HUD | Python Software Foundation License v2 | `LICENSES/python.txt` |
| Código de terceiros incorporado ao CPython (por exemplo dtoa/strtod, SipHash, Mersenne Twister, HACL\*, mimalloc, Unicode Character Database) | partes do interpretador | licenças próprias, listadas pelo CPython | `LICENSES/python-componentes.txt` |
| ncurses (`libncursesw.so.6`, `libtinfo.so.6`) | desenha a tela (`_curses`) | MIT/X11 (alguns arquivos X11 e BSD-3-Clause) | `LICENSES/ncurses.txt` |
| zlib (`libz.so.1`) | módulo `zlib` e o arquivo interno do PyInstaller | zlib | `LICENSES/zlib.txt` |
| Bootloader e runtime hooks do PyInstaller | descompacta e inicia o interpretador | GPL-2.0-or-later **com a exceção do bootloader** (permite distribuir o bootloader embutido em programas de qualquer licença); runtime hooks em Apache-2.0 | `LICENSES/pyinstaller.txt` |

As cópias de ncurses e zlib são as bibliotecas do sistema onde o binário é
montado (Ubuntu 22.04 no CI); os textos em `LICENSES/` são os arquivos
`copyright` desses pacotes. A glibc não é embutida: o binário usa a do sistema.

## O que foi deixado de fora de propósito

`packaging/hud.spec` exclui módulos que o HUD não usa, para não embutir mais
bibliotecas de terceiros: `ssl`/`_hashlib` (OpenSSL), `ctypes` (libffi),
`sqlite3`, `tkinter` (Tcl/Tk), `readline` (GPL), `xml`/`pyexpat` (expat),
`lzma`, `bz2`, `decimal` (libmpdec), `uuid` (libuuid) e `dbm`/`gdbm`.
O `hashlib` usa as implementações internas do CPython.

## Fontes

- CPython: <https://www.python.org/downloads/source/> (o CI monta com o
  Python 3.13 de `actions/setup-python`; a versão exata sai no log do build).
- ncurses: <https://invisible-island.net/ncurses/> e o pacote fonte `ncurses` do Ubuntu.
- zlib: <https://zlib.net/> e o pacote fonte `zlib` do Ubuntu.
- PyInstaller: <https://github.com/pyinstaller/pyinstaller> (versão fixada em
  `packaging/requirements-build.txt`).

O próprio HUD é distribuído sob a licença MIT (arquivo `LICENSE`, que vai no pacote e é instalado em `~/.local/bin/hud-licenses/LICENSE`).
