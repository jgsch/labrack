# Reverse engineering

How the protocols in `docs/` were found, and the tools to redo it. Everything for the analysis lives in
this folder, including its own nix flake.

The vendor installers are not in the repo. Download them from the vendor and put them in `re/installers/`
(git-ignored): `ipx-controller-setup-2.3.4b.exe` (IPX Controller) and
`LAB.GRUPPEN_IntelliDrive_Controller_64_bit_v_1_2_11_125735.msi` (IntelliDrive Controller).

| | IPX series | IPD series |
|---|---|---|
| Vendor app | IPX Controller 2.3.4b (Qt 6, C++, stripped) | IntelliDrive Controller 1.2.11 (Python 2.6 + wxPython, py2exe) |
| Method | static analysis with REA + Ghidra, then checked live on an IPX 1200 | exe unpacked, 39 modules decompiled, then checked live on an IPD2400 |
| Findings | `ipx/NOTES.md`, `ipx/paramtable.txt` (all named parameters with env/channel/item/field) | `ipd/NOTES.md` |
| Tools | `ipx/tools/ipxfile.py` (decode `.ipxpreset` / `.factory` container files) | `ipd/tools/` (below), `ipd/samples/original.icpreset` |

## Environment

`nix develop ./re` (from the repo root) gives REA, Ghidra and innoextract; the flake is `re/flake.nix`, the REA package `re/rea/`.

## IPX

`innoextract ipx-controller-setup-2.3.4b.exe`, then analyse `app/IPXController.exe` with REA/Ghidra.
`ipx/NOTES.md` lists the functions and strings the protocol was read from.

## IPD

1. `msiextract` the MSI; `IntelliDrive Controller.exe` is a py2exe bundle whose zip archive holds the
   app's `.pyc` files (Python 2.6 bytecode).
2. Decompile with `uncompyle6==3.9.2` and `xdis==6.1.1` in a venv (newer or distro builds lack the 2.6
   opcode table). All 39 modules decompile; two blocks in `d_protocol.py` are mis-indented and were fixed
   from the bytecode (`pydisasm`), see `ipd/NOTES.md`.
3. `ipd/tools/orig_harness_py27.py` loads the original packet code under Python 2.7 (needs the decompiled
   sources and a Python 2.7 interpreter, e.g. `nix build --impure github:NixOS/nixpkgs/nixos-24.05#python27`
   with `NIXPKGS_ALLOW_INSECURE=1`).
4. `IPD_SRC=<decompiled dir> IPD_PY27=<python2.7> python3 re/ipd/tools/gen_vectors.py` regenerates
   `crates/amps/tests/fixtures/ipd_vectors.json`, the golden vectors the Rust tests compare against.
