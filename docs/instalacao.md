# Instalação

> Texto para a seção "Instalar" do README.

O HUD é distribuído como um binário autocontido: o Python e o `curses` vão
dentro dele, então **não é preciso ter Python instalado**.

- **Sistemas:** Linux amd64 e arm64, com glibc 2.35 ou mais nova (Ubuntu 22.04+,
  Debian 12+, Fedora 36+ e equivalentes). No Windows, use o WSL.
- **macOS:** ainda não. O HUD lê `/proc` e `/sys`, que o macOS não tem.
- Distribuições com musl (Alpine) não rodam o binário; use o `pipx` (abaixo).

## Instalar

```bash
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash
export PATH="$HOME/.local/bin:$PATH"   # se ~/.local/bin ainda não estiver no PATH
hud --version
```

O instalador baixa `hud_linux_<arch>.tar.gz` da release, confere o SHA-256 com
o `checksums.txt` da mesma release (sem checksum válido, não instala), grava o
binário de forma atômica em `~/.local/bin/hud` e os avisos de licença de
terceiros em `~/.local/bin/hud-licenses/`. Ele só usa HTTPS (TLS 1.2+), não lê
nada do teclado e se recusa a escrever através de um symlink.

### Variáveis

| Variável | Padrão | Para quê |
|---|---|---|
| `HUD_VERSION` | `latest` | uma tag `vX.Y.Z` fixa uma versão |
| `HUD_INSTALL_DIR` | `~/.local/bin` | outra pasta de destino |
| `HUD_REPOSITORY` | `viralabs-dev/hud` | outro repositório (um fork) |

```bash
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh |
  HUD_VERSION=v0.4.0 bash
```

Para ler o script antes de rodar:

```bash
curl -fsSLO https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh
less install.sh
bash install.sh
```

## Atualizar

Rode o instalador de novo. Ele troca o binário e os avisos; a configuração
(`~/.config/hud/`) e os dados (`~/.local/share/hud/`) não são tocados.

## Desinstalar

```bash
rm ~/.local/bin/hud
rm -r ~/.local/bin/hud-licenses
```

A configuração e os dados ficam. Para apagar tudo: `rm -r ~/.config/hud ~/.local/share/hud`.

## Vindo da instalação antiga (symlink ou pipx)

Se `~/.local/bin/hud` é um symlink, por exemplo para o `bin/hud` do repositório,
o instalador para e diz o que fazer:

```text
hud: /home/voce/.local/bin/hud é um symlink (aponta para /home/voce/dev/hud/bin/hud).
     O instalador não escreve através de symlink. É a instalação antiga;
     remova o link e rode o instalador de novo:
       rm /home/voce/.local/bin/hud
```

Remova o link (o repositório não é afetado) e rode o instalador de novo:

```bash
ls -l ~/.local/bin/hud      # confira para onde aponta
rm ~/.local/bin/hud
curl -fsSL https://raw.githubusercontent.com/viralabs-dev/hud/main/install.sh | bash
```

Se a instalação antiga veio do `pipx`, use `pipx uninstall hud` no lugar do `rm`.

## Alternativa: pipx (para quem já tem Python 3.11+)

```bash
pipx install git+https://github.com/viralabs-dev/hud
```

Funciona também em distribuições sem glibc (musl). O comando `hud` instalado
roda o Python do venv em modo isolado (`-I`).

## Desenvolvimento

Do clone, sem instalar nada:

```bash
git clone https://github.com/viralabs-dev/hud
cd hud
bin/hud               # roda o código do repositório com python3 -I
python3 -m unittest discover -s tests -t .
```

Para montar o binário localmente (Linux, Python 3.11+ com `_curses` e
`objdump`):

```bash
python3 -m venv /tmp/hud-build
/tmp/hud-build/bin/pip install --require-hashes -r packaging/requirements-build.txt
PYTHON=/tmp/hud-build/bin/python scripts/release.sh v$(sed -n 's/^__version__ = "\(.*\)"$/\1/p' hud/__init__.py)
HUD_BIN=$PWD/build/pyinstaller/dist/hud python3 -m unittest tests.test_ui   # tela contra o binário
scripts/test-installer.sh dist/hud_linux_amd64.tar.gz                        # instalador contra o pacote
```

O glibc mínimo do binário é o do sistema onde ele é montado; por isso a release
é montada no Ubuntu 22.04. O binário extrai o interpretador numa pasta
temporária (`$TMPDIR`, ou `/tmp`) a cada execução e apaga ao sair; se o `/tmp`
for montado com `noexec`, aponte `TMPDIR` para uma pasta que permita execução.
