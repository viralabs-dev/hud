"""Ponto de entrada do binário autocontido (PyInstaller). Ver packaging/hud.spec."""

import sys

from hud.__main__ import main

sys.exit(main())
