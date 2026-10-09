"""Runtime hook do binário: roda antes de qualquer código do HUD.

No modo onefile, o bootloader do PyInstaller põe a pasta de extração
(sys._MEIPASS) no LD_LIBRARY_PATH do processo Python e guarda o valor original
em LD_LIBRARY_PATH_ORIG; também passa variáveis _PYI_* internas. O loader já
leu o LD_LIBRARY_PATH ao iniciar, então mexer em os.environ aqui não muda nada
neste processo; só evita que algum processo filho herde as bibliotecas
embutidas no lugar das do sistema. O HUD já monta um ambiente enxuto para os
comandos e os agentes (runner.safe_env, agent.agent_env); isto é uma camada a
mais.
"""

import os

_orig = os.environ.pop("LD_LIBRARY_PATH_ORIG", None)
if _orig is not None:
    os.environ["LD_LIBRARY_PATH"] = _orig
else:
    os.environ.pop("LD_LIBRARY_PATH", None)
for _k in [k for k in os.environ if k.startswith("_PYI_")]:
    del os.environ[_k]
del _orig

# Locale: o Python comum, ao iniciar num locale "C"/"POSIX" (LANG e LC_* vazios,
# ou um nome que o sistema não conhece) e sem LC_ALL, troca o LC_CTYPE por um
# UTF-8 (PEP 538: C.UTF-8, C.utf8 ou UTF-8, o primeiro que existir) e exporta
# LC_CTYPE. O bootloader do PyInstaller inicia o Python em modo isolado, que pula
# essa etapa; sem ela o curses fica em ASCII e desenha bordas, "·" e acentos como
# espaço. Isto repete a PEP 538 antes do hud.__main__ chamar setlocale(LC_ALL, "").
if os.name == "posix" and not os.environ.get("LC_ALL"):
    import locale as _locale

    try:
        _locale.setlocale(_locale.LC_CTYPE, "")
    except _locale.Error:
        pass
    if _locale.setlocale(_locale.LC_CTYPE) in ("C", "POSIX"):
        for _alvo in ("C.UTF-8", "C.utf8", "UTF-8"):
            try:
                _locale.setlocale(_locale.LC_CTYPE, _alvo)
            except _locale.Error:
                continue
            os.environ["LC_CTYPE"] = _alvo
            break
    del _locale
