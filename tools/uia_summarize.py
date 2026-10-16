"""Synthèse structurelle d'un relevé UIA (TSV de tools/uia_dump.ps1), sans données personnelles.

Garde : ruban (onglets, groupes, commandes, KeyTips, raccourcis), barre d'accès rapide,
navigation, barre d'état, en-têtes de colonnes. Exclut : contenus de listes, arbres, documents.
Usage : python -I tools/uia_summarize.py notes/uia-raw/<relevé>.tsv [...]
"""
import csv
import sys

KEEP_ROOTS = ("Ribbon", "Barre d'état", "Navigation Bar", "FolderBar")
DROP_IDS = {"MeControlWidget"}
DROP_TYPES = {"ListItem", "DataItem", "TreeItem", "Document", "Hyperlink", "Text", "Image", "Thumb", "ScrollBar"}
DROP_SUBTREES = {"Tree", "Document", "List"}


def summarize(path: str) -> list[str]:
    rows = list(csv.DictReader(open(path, encoding="utf-8"), delimiter="\t"))
    out, keep_depth, drop_depth = [], None, None
    for r in rows:
        d = int(r["depth"])
        if keep_depth is not None and d <= keep_depth:
            keep_depth = None
        if drop_depth is not None and d <= drop_depth:
            drop_depth = None
        name, ctype = r["name"], r["controlType"]
        if keep_depth is None and (name in KEEP_ROOTS or r["className"] in KEEP_ROOTS or ctype == "Table"):
            keep_depth = d
        if keep_depth is None or drop_depth is not None:
            continue
        if ctype in DROP_SUBTREES:
            drop_depth = d
            continue
        if r["automationId"] in DROP_IDS:
            continue
        if ctype in DROP_TYPES or (ctype == "Group" and r["className"] == "GroupHeader"):
            continue
        if ctype == "Pane" and not name:
            continue
        extra = []
        if r["automationId"] and not r["automationId"].isdigit():
            extra.append(f"id={r['automationId']}")
        if r["accessKey"]:
            extra.append(f"touches={r['accessKey']}")
        if r["acceleratorKey"]:
            extra.append(f"raccourci={r['acceleratorKey']}")
        if r["enabled"] == "False":
            extra.append("désactivé")
        out.append(f"{'  ' * (d - keep_depth)}- {ctype} « {name} »" + (f" ({', '.join(extra)})" if extra else ""))
    return out


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    for p in sys.argv[1:]:
        print(f"\n### {p.rsplit('/', 1)[-1].rsplit(chr(92), 1)[-1]}\n")
        print("\n".join(summarize(p)))
