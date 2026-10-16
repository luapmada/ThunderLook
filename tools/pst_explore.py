"""Statistiques structurelles d'un PST (copie) via libpff, en lecture seule.

Usage : .venv/Scripts/python.exe -I tools/pst_explore.py <copie.pst> <sortie.json>
La sortie peut contenir des noms de dossiers personnels : à garder dans data/ (ignoré par git).
"""
import collections
import json
import sys
import time

import pypff

TAG_CLASS = 0x001A
TAG_BODY = 0x1000
TAG_RTF = 0x1009
TAG_HTML = 0x1013


def entries(item):
    for rs in range(item.number_of_record_sets):
        rec = item.get_record_set(rs)
        for e in range(rec.number_of_entries):
            yield rec.get_entry(e)


def main(src: str, dst: str) -> None:
    t0 = time.time()
    f = pypff.file()
    f.open(src)
    stats = {
        "fichier": src,
        "taille": f.size,
        "encryption_type": f.encryption_type,
        "folders": [],
        "message_classes": collections.Counter(),
        "body_kinds": collections.Counter(),
        "prop_tags": collections.Counter(),
        "prop_types": collections.Counter(),
        "attachments": 0,
        "messages": 0,
        "erreurs": 0,
        "orphans": f.number_of_orphan_items,
    }

    def walk(folder, path):
        stats["folders"].append({"path": path, "messages": folder.number_of_sub_messages,
                                 "sub_folders": folder.number_of_sub_folders})
        for i in range(folder.number_of_sub_messages):
            try:
                m = folder.get_sub_message(i)
                tags = set()
                cls = None
                for e in entries(m):
                    tag = e.entry_type
                    tags.add(tag)
                    stats["prop_tags"][f"0x{tag:04X}"] += 1
                    stats["prop_types"][f"0x{e.value_type:04X}"] += 1
                    if tag == TAG_CLASS:
                        try:
                            cls = e.get_data_as_string()
                        except Exception:
                            cls = "?"
                stats["message_classes"][cls or "(absent)"] += 1
                kind = "+".join(k for k, t in (("html", TAG_HTML), ("rtf", TAG_RTF), ("text", TAG_BODY)) if t in tags)
                stats["body_kinds"][kind or "aucun"] += 1
                stats["attachments"] += m.number_of_attachments
                stats["messages"] += 1
            except Exception:
                stats["erreurs"] += 1
        for i in range(folder.number_of_sub_folders):
            sub = folder.get_sub_folder(i)
            walk(sub, f"{path}/{sub.name}")

    walk(f.get_root_folder(), "")
    stats["duree_s"] = round(time.time() - t0, 1)
    for k in ("message_classes", "body_kinds", "prop_tags", "prop_types"):
        stats[k] = dict(stats[k].most_common())
    with open(dst, "w", encoding="utf-8") as out:
        json.dump(stats, out, ensure_ascii=False, indent=1)
    print(f"messages={stats['messages']} dossiers={len(stats['folders'])} pj={stats['attachments']} "
          f"erreurs={stats['erreurs']} orphelins={stats['orphans']} duree={stats['duree_s']}s")


if __name__ == "__main__":
    sys.stdout.reconfigure(encoding="utf-8")
    main(sys.argv[1], sys.argv[2])
