# SPDX-License-Identifier: MIT OR Apache-2.0
"""Type stubs for the Pite engine's embedded ``pite`` module.

The real module is native: the engine registers it from Rust through PyO3
inside the CPython interpreter it embeds. It cannot be installed or imported
from a normal Python environment, so this distribution (``pite-stubs`` on
PyPI) ships ``__init__.pyi`` only.

Install it as a development dependency so Pylance/Pyright resolve
``import pite`` and offer autocomplete outside the Pite editor. Run the
project with ``pite edit`` / ``pite run``.
"""

raise ImportError(
    "The `pite` module is provided by the Pite engine's embedded interpreter, "
    "not by pip. This package ships type stubs only: install it as a dev "
    "dependency for autocomplete, and run the project with `pite edit` or "
    "`pite run`. See https://github.com/piteengine/pite"
)
