# SPDX-License-Identifier: MIT OR Apache-2.0
"""Source sibling for the shipped `pite.pyi` stubs.

The real `pite` module is native, registered from Rust via PyO3
(`append_to_inittab!`); there is no Python implementation. This file exists
only so type checkers resolve `import pite` to a source file — types always
come from `pite.pyi`. Never imported at runtime; never put on `sys.path`.
"""
