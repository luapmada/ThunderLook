"""Decode le catalogue d'elements de regle extrait d'OUTLOOK.EXE (notes/rea/rule_catalog.hex).

Usage : python -I tools/rule_catalog.py [--dll <OUTLLIBR.DLL>] [fichier.hex]
Avec --dll, ajoute le libelle de chaque element (ressource chargee en donnees, sans executer la DLL).
"""
import struct
import sys
from pathlib import Path

EXPECTED = {"header": 1, "apply": 1, "conditions": 37, "actions": 35, "exceptions": 30}
ENTRY = 0x30


def load(path):
    out = {}
    for line in Path(path).read_text(encoding="ascii").splitlines():
        if not line.strip() or line.startswith("#"):
            continue
        section, _addr, hx = line.split()
        raw = bytes.fromhex(hx)
        assert len(raw) == EXPECTED[section] * ENTRY, (section, len(raw))
        out[section] = sorted(struct.unpack_from("<12I", raw, i * ENTRY) for i in range(len(raw) // ENTRY))
    return out


def string_loader(dll):
    import ctypes
    from ctypes import wintypes

    k32 = ctypes.WinDLL("kernel32", use_last_error=True)
    u32 = ctypes.WinDLL("user32")
    k32.LoadLibraryExW.restype = wintypes.HMODULE
    k32.LoadLibraryExW.argtypes = [wintypes.LPCWSTR, wintypes.HANDLE, wintypes.DWORD]
    u32.LoadStringW.argtypes = [wintypes.HMODULE, wintypes.UINT, wintypes.LPWSTR, ctypes.c_int]
    # LOAD_LIBRARY_AS_DATAFILE | LOAD_LIBRARY_AS_IMAGE_RESOURCE
    module = k32.LoadLibraryExW(str(dll), None, 0x2 | 0x20)
    if not module:
        raise OSError(ctypes.get_last_error(), f"chargement impossible : {dll}")
    buf = ctypes.create_unicode_buffer(2048)
    return lambda sid: buf.value if u32.LoadStringW(module, sid, buf, len(buf)) else ""


def main(argv):
    dll = None
    if "--dll" in argv:
        i = argv.index("--dll")
        dll = argv[i + 1]
        del argv[i:i + 2]
    path = argv[0] if argv else Path(__file__).parent.parent / "notes/rea/rule_catalog.hex"
    label = string_loader(dll) if dll else (lambda sid: "")
    for section, rows in load(path).items():
        print(f"### {section} ({len(rows)})\n")
        print("| id | type | f3 | f4 | f5 | f6-f9 | chaine | libelle |")
        print("|---|---|---|---|---|---|---|---|")
        for r in rows:
            text = label(r[10]).replace("|", "\\|")
            print(f"| {r[0]:#06x} ({r[0]}) | {r[2]:#04x} | {r[3]:#04x} | {r[4]:#06x} "
                  f"| {r[5] & 0xffff}/{r[5] >> 16} | {r[6]},{r[7]},{r[8]},{r[9]} | {r[10]:#06x} | {text} |")
        print()


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main(sys.argv[1:])
