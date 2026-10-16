"""Lit l'en-tête (HEADER) de fichiers PST/OST d'après [MS-PST] 2.2.2.6.

Usage : python -I tools/pst_header.py <fichier.hdr|.pst> [...]
Lecture seule. Source : https://learn.microsoft.com/en-us/openspecs/office_file_formats/ms-pst/c9876f5a-664b-46a3-9887-ba63f113abf5
"""
import struct
import sys
from pathlib import Path

CRYPT = {0x00: "NONE", 0x01: "PERMUTE", 0x02: "CYCLIC", 0x10: "EDPCRYPTED(WIP)"}


def parse(data: bytes) -> dict:
    if data[:4] != b"!BDN":
        return {"erreur": "dwMagic absent"}
    client = data[8:10].decode("ascii", "replace")
    w_ver, w_ver_client = struct.unpack_from("<HH", data, 10)
    out = {"client": client, "wVer": w_ver, "wVerClient": w_ver_client}
    if w_ver in (14, 15):
        out["format"] = "ANSI"
        out["dwUnique"] = struct.unpack_from("<I", data, 32)[0]
        out["ibFileEof"] = struct.unpack_from("<I", data, 164 + 4)[0]
        sentinel, crypt = data[460], data[461]
    else:
        out["format"] = "Unicode" + (" 4K" if w_ver >= 36 else "")
        out["dwUnique"] = struct.unpack_from("<I", data, 40)[0]
        out["ibFileEof"] = struct.unpack_from("<Q", data, 180 + 4)[0]
        sentinel, crypt = data[512], data[513]
    out["bSentinel_ok"] = sentinel == 0x80
    out["bCryptMethod"] = CRYPT.get(crypt, hex(crypt))
    return out


def main() -> None:
    for arg in sys.argv[1:]:
        p = Path(arg)
        with p.open("rb") as f:
            info = parse(f.read(4096))
        fields = " ".join(f"{k}={v}" for k, v in info.items())
        print(f"{p.name}: {fields}")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main()
