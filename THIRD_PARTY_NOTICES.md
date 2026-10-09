# Avisos de terceiros

O código-fonte do HUD não tem dependências: só a biblioteca padrão do Python e o
módulo `curses`. Este arquivo vale para o **binário autocontido** dos pacotes
das releases (`hud_linux_<arch>.tar.gz`, `hud_darwin_<arch>.tar.gz` e
`hud_windows_amd64.zip`), que embute os componentes abaixo. Os textos completos
ficam em `LICENSES/`, dentro do pacote e, depois de instalar, em
`~/.local/bin/hud-licenses/` (Linux e macOS) ou
`%LOCALAPPDATA%\Programs\hud\hud-licenses\` (Windows). Todo pacote leva todos os
textos; as tabelas dizem o que cada binário embute de fato.

Na release, `packaging/bibliotecas.py` lista cada biblioteca nativa embutida no
binário e falha se alguma não tiver regra e texto de licença aqui.

## Em todas as plataformas

| Componente | Para que serve no binário | Licença | Texto |
|---|---|---|---|
| CPython 3.13 (interpretador, biblioteca padrão e extensões nativas) | roda o HUD | Python Software Foundation License v2 | `LICENSES/python.txt` |
| Código de terceiros incorporado ao CPython (por exemplo dtoa/strtod, SipHash, Mersenne Twister, HACL\*, mimalloc, Unicode Character Database) | partes do interpretador | licenças próprias, listadas pelo CPython | `LICENSES/python-componentes.txt` |
| Bootloader e runtime hooks do PyInstaller | descompacta e inicia o interpretador | GPL-2.0-or-later **com a exceção do bootloader** (permite distribuir o bootloader embutido em programas de qualquer licença); runtime hooks em Apache-2.0 | `LICENSES/pyinstaller.txt` |

## Linux (`hud_linux_amd64`, `hud_linux_arm64`)

| Componente | Para que serve no binário | Licença | Texto |
|---|---|---|---|
| `libpython3.13.so` e as extensões de `lib-dynload` | CPython (acima) | PSF-2.0 | `LICENSES/python.txt` |
| ncurses (`libncursesw.so.6`, `libtinfo.so.6`) | desenha a tela (`_curses`) | MIT/X11 (alguns arquivos X11 e BSD-3-Clause) | `LICENSES/ncurses.txt` |
| zlib (`libz.so.1`) | módulo `zlib` e o arquivo interno do PyInstaller | zlib | `LICENSES/zlib.txt` |

As cópias de ncurses e zlib são as bibliotecas do sistema onde o binário é
montado (Ubuntu 22.04 no CI); os textos em `LICENSES/` são os arquivos
`copyright` desses pacotes. A glibc não é embutida: o binário usa a do sistema.

## macOS (`hud_darwin_arm64`, `hud_darwin_amd64`)

| Componente | Para que serve no binário | Licença | Texto |
|---|---|---|---|
| `Python` (framework do Python.org, via `actions/setup-python`) e as extensões de `lib-dynload` | CPython (acima) | PSF-2.0 | `LICENSES/python.txt` |
| ncurses que vem com o Python.org (`libncursesw.*.dylib`, se o build a embutir) | desenha a tela (`_curses`) | MIT/X11 | `LICENSES/ncurses.txt` |
| `ctypes` (extensão `_ctypes` do CPython) | leitura de indicadores do sistema (sysctl, Mach) | PSF-2.0 | `LICENSES/python.txt` |

A libffi, a zlib e as bibliotecas do sistema (`libSystem`, `/usr/lib/libncurses`
se for a usada) vêm do macOS e não são embutidas. O texto de `LICENSES/ncurses.txt`
é o do pacote do Ubuntu; a licença do ncurses é a mesma (MIT/X11) em qualquer
distribuição. O binário não é assinado com um Developer ID nem notarizado pela
Apple (o PyInstaller só faz a assinatura ad hoc que o Apple Silicon exige).

## Windows (`hud_windows_amd64`)

| Componente | Para que serve no binário | Licença | Texto |
|---|---|---|---|
| `python313.dll` (Python.org) e as extensões `.pyd` da biblioteca padrão | CPython (acima); a zlib vem ligada estaticamente na DLL | PSF-2.0 (zlib: licença zlib) | `LICENSES/python.txt`, `LICENSES/python-componentes.txt`, `LICENSES/zlib.txt` |
| windows-curses 2.4.2 (`_curses`, `_curses_panel`) | módulo `curses` no Windows | Python Software Foundation License v2 (deriva do `_cursesmodule.c` do CPython) | `LICENSES/windows-curses.txt` |
| PDCurses (ligado estaticamente nas `.pyd` do windows-curses; o wheel não traz DLL separada) | desenha a tela no console do Windows | domínio público (núcleo e porte `wincon`) | `LICENSES/pdcurses.txt` |
| libffi (`libffi-8.dll`) | `ctypes`, para ler indicadores do sistema (kernel32, psapi) | MIT | `LICENSES/libffi.txt` |
| Runtime do Visual C++ (`VCRUNTIME140.dll`, `VCRUNTIME140_1.dll`) | runtime C do `python313.dll` | redistribuível da Microsoft (termos do Visual Studio para "Distributable Code"); sem texto próprio aqui | — |

O Universal C Runtime (`ucrtbase.dll`, `api-ms-win-*.dll`) é parte do Windows 10
e 11 e não é embutido. O `hud.exe` não é assinado digitalmente: o SmartScreen
pode avisar se o arquivo vier pelo navegador, e um antivírus pode estranhar o
formato onefile do PyInstaller (que se extrai em `%TEMP%` a cada execução).

## O que foi deixado de fora de propósito

`packaging/hud.spec` exclui módulos que o HUD não usa, para não embutir mais
bibliotecas de terceiros: `ssl`/`_hashlib` (OpenSSL), `sqlite3`, `tkinter`
(Tcl/Tk), `readline` (GPL), `xml`/`pyexpat` (expat), `lzma`, `bz2`, `decimal`
(libmpdec), `uuid` (libuuid) e `dbm`/`gdbm`. No Linux também ficam fora o
`ctypes` (libffi) e o painel do curses (`libpanelw`); no macOS, o painel do
curses. O `hashlib` usa as implementações internas do CPython.

## Fontes

- CPython: <https://www.python.org/downloads/source/> (o CI monta com o
  Python 3.13 de `actions/setup-python`; a versão exata sai no log do build).
- ncurses: <https://invisible-island.net/ncurses/> e o pacote fonte `ncurses` do Ubuntu.
- zlib: <https://zlib.net/> e o pacote fonte `zlib` do Ubuntu.
- windows-curses: <https://github.com/zephyrproject-rtos/windows-curses> (versão
  e hashes em `packaging/requirements-build-windows.txt`).
- PDCurses: <https://pdcurses.org/> e <https://github.com/zephyrproject-rtos/PDCurses>.
- libffi: <https://github.com/libffi/libffi> (a versão é a que vem com o CPython do Python.org).
- PyInstaller: <https://github.com/pyinstaller/pyinstaller> (versão fixada em
  `packaging/requirements-build*.txt`).

O próprio HUD é distribuído sob a licença MIT (arquivo `LICENSE`, que vai no
pacote e é instalado junto dos avisos, em `hud-licenses/LICENSE`).
