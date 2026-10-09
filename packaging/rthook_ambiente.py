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
