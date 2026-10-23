#!/usr/bin/env node
// Contrôle LECTURE SEULE : dossier de données de MPE <-> dossier d'échange ThunderLook.
//
//   node tools/mpe_check.mjs [dossier_mpe] [dossier_echange] [--max N]
//
// dossier_mpe  : contient Phonebook.dat, Organizer.dat, Notes.dat. Défaut : %APPDATA%\MyPhoneExplorer\<appareil>
//                (si plusieurs appareils, le dire explicitement : leurs noms sont listés).
// dossier_echange : contient contacts.vcf, calendrier.ics, taches.ics, notes.vnt, manifest.json (crates/exchange).
//
// Sortie : compteurs et identifiants seulement ; jamais le contenu des fiches ni des notes.
// Code de sortie : 0 = rien à signaler, 1 = écart, doublon ou élément d'un seul côté, 2 = erreur d'usage ou de lecture.
// Le script ne fait que lire (readFileSync/readdirSync/statSync) : il n'écrit, ne crée, ne supprime rien, ne lance rien.

import { readFileSync, readdirSync, statSync, existsSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { join } from 'node:path';

const KINDS = [
  { key: 'contacts', label: 'contacts', mpe: 'Phonebook.dat', ex: 'contacts.vcf', tag: 'VCARD' },
  { key: 'events', label: 'rendez-vous', mpe: 'Organizer.dat', ex: 'calendrier.ics', tag: 'VEVENT' },
  { key: 'notes', label: 'notes', mpe: 'Notes.dat', ex: 'notes.vnt', tag: 'VNOTE' },
  { key: 'tasks', label: 'tâches', mpe: 'Organizer.dat', ex: 'taches.ics', tag: 'VTODO' },
];

function fail(msg) {
  console.error(`mpe_check: ${msg}`);
  process.exit(2);
}

function parseArgs(argv) {
  const pos = [];
  let max = 10;
  for (let i = 0; i < argv.length; i++) {
    if (argv[i] === '--max') {
      max = Number(argv[++i]);
      if (!Number.isInteger(max) || max < 0) fail('--max attend un entier >= 0');
    } else if (argv[i] === '-h' || argv[i] === '--help') {
      console.log('usage : node tools/mpe_check.mjs [dossier_mpe] [dossier_echange] [--max N]');
      process.exit(0);
    } else pos.push(argv[i]);
  }
  if (pos.length > 2) fail('trop d\'arguments');
  return { pos, max };
}

function isDir(p) {
  try { return statSync(p).isDirectory(); } catch { return false; }
}

function defaultMpeDir() {
  const appdata = process.env.APPDATA;
  if (!appdata) fail('pas de dossier MPE : passer le chemin en argument (APPDATA absent)');
  const root = join(appdata, 'MyPhoneExplorer');
  if (!isDir(root)) fail(`introuvable : ${root}`);
  const devices = readdirSync(root, { withFileTypes: true })
    .filter((d) => d.isDirectory() && existsSync(join(root, d.name, 'Phonebook.dat')))
    .map((d) => d.name);
  if (devices.length === 1) return join(root, devices[0]);
  fail(devices.length ? `plusieurs appareils, passer le dossier voulu : ${devices.join(' | ')}` : `aucun appareil (Phonebook.dat) sous ${root}`);
}

// Texte brut lu en latin1 : un octet = un caractère, rien n'est interprété (les identifiants sont ASCII).
function readText(path) {
  return readFileSync(path).toString('latin1');
}

// Lignes dépliées (RFC 2425/5545 : une ligne qui commence par espace ou tabulation prolonge la précédente).
function unfold(text) {
  const out = [];
  for (const raw of text.split(/\r\n|\n|\r/)) {
    if ((raw.startsWith(' ') || raw.startsWith('\t')) && out.length) out[out.length - 1] += raw.slice(1);
    else out.push(raw);
  }
  return out;
}

// Propriété `NOM[;params]:valeur` -> [NOM en majuscules, valeur] ; la valeur n'est lue que pour les identifiants.
function prop(line) {
  const i = line.indexOf(':');
  if (i < 1) return null;
  return [line.slice(0, i).split(';')[0].toUpperCase(), line.slice(i + 1).trim()];
}

// Éléments de premier niveau portant `tag` (les VALARM et autres sous-composants sont ignorés).
// Retourne [{ id, rid }] : id = identifiant retenu (ou null), rid = RECURRENCE-ID (occurrence modifiée) ou null.
function scan(text, tag, idProps) {
  const items = [];
  let cur = null;
  let depth = 0;
  for (const line of unfold(text)) {
    const p = prop(line);
    if (!p) continue;
    const [name, value] = p;
    const v = value.toUpperCase();
    if (name === 'BEGIN') {
      if (cur) depth++;
      else if (v === tag) cur = { props: {}, rid: null };
    } else if (name === 'END') {
      if (!cur) continue;
      if (depth > 0) depth--;
      else if (v === tag) {
        const id = idProps.map((n) => cur.props[n]).find((x) => x) ?? null;
        items.push({ id, rid: cur.rid });
        cur = null;
      }
    } else if (cur && depth === 0) {
      if (name === 'RECURRENCE-ID') cur.rid = value || '?';
      else if (idProps.includes(name) && value && !cur.props[name]) cur.props[name] = value;
    }
  }
  return items;
}

// Propriétés d'identité, par côté (EntryID du PST côté MPE, identifiant ThunderLook ou de MPE côté échange).
const ID_PROPS = {
  mpe: { VCARD: ['OUTLOOK-UID', 'X-OUTLOOK-UID', 'X-IRMC-LUID', 'UID'], VEVENT: ['X-OUTLOOK-UID', 'UID'], VTODO: ['X-OUTLOOK-UID', 'UID'], VNOTE: ['X-OUTLOOK-UID', 'UID'] },
  ex: { VCARD: ['UID', 'X-OUTLOOK-UID'], VEVENT: ['UID', 'X-OUTLOOK-UID'], VTODO: ['UID', 'X-OUTLOOK-UID'], VNOTE: ['X-OUTLOOK-UID', 'UID'] },
};

function analyse(path, tag, side) {
  if (!existsSync(path)) return { present: false };
  const items = scan(readText(path), tag, ID_PROPS[side][tag]);
  // Une occurrence modifiée d'une série (RECURRENCE-ID) partage l'UID de sa série : elle ne compte pas comme élément.
  const base = items.filter((x) => !x.rid);
  const ids = new Map();
  let noId = 0;
  for (const x of items) {
    if (!x.id) { noId++; continue; }
    const k = x.rid ? `${x.id}#${x.rid}` : x.id;
    ids.set(k, (ids.get(k) ?? 0) + 1);
  }
  const dups = [...ids].filter(([, n]) => n > 1).map(([k, n]) => ({ id: k, n }));
  const set = new Set(base.filter((x) => x.id).map((x) => x.id));
  return { present: true, count: base.length, overrides: items.length - base.length, noId, dups, set };
}

function sha256(path) {
  return createHash('sha256').update(readFileSync(path)).digest('hex');
}

function shown(list, max) {
  const head = list.slice(0, max).join(' ');
  return list.length > max ? `${head} … (+${list.length - max})` : head;
}

const { pos, max } = parseArgs(process.argv.slice(2));
const mpeDir = pos[0] ?? defaultMpeDir();
const exDir = pos[1];
if (!exDir) fail('dossier d\'échange manquant : node tools/mpe_check.mjs [dossier_mpe] <dossier_echange>');
if (!isDir(mpeDir)) fail(`dossier MPE introuvable : ${mpeDir}`);
if (!isDir(exDir)) fail(`dossier d'échange introuvable : ${exDir}`);

let problems = 0;
let anyFile = false;
const lines = [];
const note = (s) => lines.push(s);
const flag = (s) => { problems++; lines.push(`  ! ${s}`); };

for (const k of KINDS) {
  const a = analyse(join(mpeDir, k.mpe), k.tag, 'mpe');
  const b = analyse(join(exDir, k.ex), k.tag, 'ex');
  anyFile ||= a.present || b.present;
  note(`${k.label} : MPE ${a.present ? a.count : 'absent'} | échange ${b.present ? b.count : 'absent'}`);
  if (!a.present || !b.present) {
    flag(`fichier absent côté ${!a.present ? `MPE (${k.mpe})` : `échange (${k.ex})`}`);
    continue;
  }
  if (a.count !== b.count) flag(`différence de nombre : ${a.count - b.count > 0 ? '+' : ''}${a.count - b.count} (MPE − échange)${k.key === 'events' ? ' ; la plage de MPE est limitée (−90/+90 jours par défaut) [supposé : ThunderLook exporte tout]' : ''}`);
  for (const [side, r, file] of [['MPE', a, k.mpe], ['échange', b, k.ex]]) {
    if (r.dups.length) flag(`UID dupliqués côté ${side} (${file}) : ${r.dups.length} — ${shown(r.dups.map((d) => `${d.id}×${d.n}`), max)}`);
    if (r.noId) flag(`${r.noId} élément(s) sans identifiant côté ${side} (${file})`);
  }
  if (a.overrides || b.overrides) note(`  occurrences modifiées de séries (hors compte) : MPE ${a.overrides} | échange ${b.overrides}`);
  const onlyMpe = [...a.set].filter((x) => !b.set.has(x));
  const onlyEx = [...b.set].filter((x) => !a.set.has(x));
  if (a.set.size && b.set.size && onlyMpe.length === a.set.size && onlyEx.length === b.set.size) {
    flag('aucun identifiant commun : identifiants incomparables (EntryID du PST côté MPE, identifiant ThunderLook côté échange ?) — présence unilatérale non évaluée [supposé]');
  } else {
    if (onlyMpe.length) flag(`${onlyMpe.length} présent(s) seulement côté MPE : ${shown(onlyMpe, max)}`);
    if (onlyEx.length) flag(`${onlyEx.length} présent(s) seulement côté échange : ${shown(onlyEx, max)}`);
  }
}

// manifest.json : nombre et empreinte écrits à l'export, comparés à l'état actuel des fichiers (le téléphone les a-t-il changés ?).
const manifestPath = join(exDir, 'manifest.json');
if (existsSync(manifestPath)) {
  let m = null;
  try { m = JSON.parse(readText(manifestPath)); } catch { /* signalé ci-dessous */ }
  if (!m || !Array.isArray(m.files)) flag('manifest.json illisible');
  else {
    note(`manifest : exporté le ${m.exported_at ?? '?'}`);
    for (const f of m.files) {
      const p = join(exDir, String(f.name));
      if (!existsSync(p)) { flag(`manifest : ${f.name} absent du dossier`); continue; }
      if (sha256(p) !== f.sha256) note(`  ${f.name} modifié depuis l'export (empreinte différente) — normal après une synchro MPE, à relire dans ThunderLook avant tout nouvel export`);
    }
  }
} else note('manifest.json absent (aucune suppression ne sera proposée par ThunderLook)');

if (!anyFile) fail('aucun fichier reconnu dans les deux dossiers');
console.log(`mpe_check (lecture seule)\n  MPE : ${mpeDir}\n  échange : ${exDir}`);
console.log(lines.join('\n'));
console.log(problems ? `RÉSULTAT : ${problems} point(s) à examiner` : 'RÉSULTAT : rien à signaler');
process.exit(problems ? 1 : 0);
