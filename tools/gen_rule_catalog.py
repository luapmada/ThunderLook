"""Génère crates/rules/src/catalog.rs à partir de notes/rea/rule_catalog.md (catalogue extrait d'OUTLOOK.EXE).
Usage : python -I tools/gen_rule_catalog.py"""
import pathlib, re

root = pathlib.Path(__file__).resolve().parent.parent
text = (root / 'notes/rea/rule_catalog.md').read_text(encoding='utf-8')
section = None
rows = []
for line in text.splitlines():
    m = re.match(r'^### (\w+)', line)
    if m:
        section = m.group(1)
        continue
    m = re.match(r'^\| (0x[0-9a-f]{4}) \(\d+\) \| (0x[0-9a-f]{2}) \|.*\| (0x[0-9a-f]{4}) \| (.*) \|$', line)
    if m:
        rows.append((int(m.group(1), 16), int(m.group(2), 16), section, m.group(4).replace('\\|', '|')))

kind = {'header': 'Header', 'apply': 'Header', 'conditions': 'Condition', 'actions': 'Action', 'exceptions': 'Exception'}
out = [
    '//! Catalogue des éléments de règle d\'Outlook 2021 (identifiant, type de valeur, nature, libellé français).',
    '//! Généré par `tools/gen_rule_catalog.py` depuis `notes/rea/rule_catalog.md` (extrait d\'OUTLOOK.EXE et',
    '//! d\'OUTLLIBR.DLL par REA). Ne pas modifier à la main.',
    '',
    'use crate::Kind;',
    '',
    '/// (identifiant, type de valeur, nature, libellé avec `%s` pour la valeur).',
    'pub const CATALOG: &[(u32, u8, Kind, &str)] = &[',
]
for eid, ty, sec, label in sorted(rows):
    literal = '"' + label.replace('\\', '\\\\').replace('"', '\\"') + '"'
    out.append(f'    ({eid:#06x}, {ty:#04x}, Kind::{kind[sec]}, {literal}),')
out.append('];')
out.append('')
(root / 'crates/rules/src').mkdir(parents=True, exist_ok=True)
(root / 'crates/rules/src/catalog.rs').write_text('\n'.join(out), encoding='utf-8')
print(len(rows), 'éléments')
