// Cas L-rendez-vous : créer ; série hebdomadaire de 4 occurrences ; modifier UNE occurrence, puis toute la série ;
// supprimer une occurrence, puis la série. Vue Semaine, deux semaines après la semaine courante pour la première occurrence.
// Écrit dans la base : exige THUNDERLOOK_DATA. Tout porte « ZZ-L… » ; le balayage final supprime ce qui resterait.
import { js, until, sleep, setValue, setSelect, mustClick, requireTestData, tag, ymd } from './lib.mjs';
import { openCalendar, setKind, weekAhead, nextPeriod, chips, selectChip, openSelected, deleteSelected, createAppointment, saveDialog, sweepAppointments, currentKind } from './data.mjs';

export const name = 'L-rendez-vous : créer, série hebdomadaire ×4, modifier une occurrence / la série, supprimer une occurrence / la série';

/** Nombre de rendez-vous « prefix… » affichés dans chacune des semaines `from` à `from + n - 1` (à partir de la semaine courante). */
async function perWeek(prefix, from, n) {
  await weekAhead(from);
  const out = [];
  for (let i = 0; i < n; i++) {
    out.push((await chips(prefix)).length);
    if (i < n - 1) await nextPeriod();
  }
  return out;
}
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

export async function run(log) {
  if (!requireTestData(log)) return;
  const single = tag('r');
  const series = tag('s');
  let kind0 = null;
  try {
    await openCalendar();
    kind0 = await currentKind();
    await setKind('Semaine');

    // 1. Un rendez-vous simple dans 14 jours (même jour de semaine qu'aujourd'hui : exactement deux semaines plus loin).
    await createAppointment({ subject: single, date: ymd(14), from: '14:00', to: '15:00', location: 'ZZ salle' });
    await weekAhead(2);
    const one = await chips(single);
    if (one.length !== 1) throw new Error(`rendez-vous simple : ${one.length} affiché(s) au lieu de 1 (${one.join(' | ')})`);
    log(`créé : « ${one[0]} »`);
    await selectChip(single);
    await deleteSelected(false);
    if ((await chips(single)).length !== 0) throw new Error('rendez-vous simple encore affiché après suppression');
    log('supprimé : disparu de la semaine');

    // 2. Série hebdomadaire de 4 occurrences : semaines +2, +3, +4, +5 (et rien en +6).
    await createAppointment({ subject: series, date: ymd(14), from: '10:00', to: '11:00', repeat: 'weekly', count: 4 });
    let counts = await perWeek(series, 2, 5);
    if (!same(counts, [1, 1, 1, 1, 0])) throw new Error(`série ×4 : occurrences par semaine (+2 à +6) = ${counts.join(',')}, attendu 1,1,1,1,0`);
    log('série créée : 4 occurrences (semaines +2 à +5), aucune en +6');

    // 3. Modifier UNE occurrence (la 2e : objet).
    await weekAhead(3);
    await selectChip(series);
    await openSelected();
    const scope = await js(`document.querySelector('.dialog select[aria-label="Appliquer à"]')?.value ?? null`);
    log(`occurrence ouverte : « Appliquer à » = ${scope ?? '(absent)'}`);
    if (scope !== null && scope !== 'occurrence') await setSelect('.dialog select[aria-label="Appliquer à"]', 'occurrence');
    await setValue('.dialog input[aria-label="Objet"]', `${series} modifiée`);
    await saveDialog('modification d\'une occurrence');
    await sleep(500);
    counts = await perWeek(`${series} modifiée`, 2, 4);
    if (!same(counts, [0, 1, 0, 0])) throw new Error(`occurrence modifiée : « modifiée » par semaine (+2 à +5) = ${counts.join(',')}, attendu 0,1,0,0`);
    const others = await perWeek(`${series},`, 2, 4);
    log(`une occurrence modifiée (semaine +3) ; autres occurrences inchangées : ${others.join(',')}`);
    if (others[1] !== 0 || others[0] + others[2] + others[3] !== 3) throw new Error(`les 3 autres occurrences devraient garder l'objet d'origine (${others.join(',')})`);

    // 4. Modifier toute la série (emplacement).
    await weekAhead(2);
    await selectChip(series);
    await openSelected();
    await until(`!!document.querySelector('.dialog select[aria-label="Appliquer à"]')`, 'liste « Appliquer à »');
    await setSelect('.dialog select[aria-label="Appliquer à"]', 'series');
    await setValue('.dialog input[aria-label="Emplacement"]', 'ZZ lieu série');
    await saveDialog('modification de la série');
    await sleep(500);
    await weekAhead(2);
    const located = [];
    for (let i = 0; i < 4; i++) {
      located.push((await chips(series)).filter((l) => l.includes('ZZ lieu série')).length);
      if (i < 3) await nextPeriod();
    }
    log(`emplacement de la série par semaine (+2 à +5) : ${located.join(',')} (l'occurrence déjà modifiée peut avoir été remise à la série : à regarder)`);
    if (located[0] !== 1 || located[2] !== 1 || located[3] !== 1) throw new Error(`l'emplacement n'a pas été appliqué aux occurrences non modifiées (${located.join(',')})`);

    // 5. Supprimer UNE occurrence (la 3e : semaine +4).
    await weekAhead(4);
    await selectChip(series);
    await deleteSelected(false);
    counts = await perWeek(series, 2, 4);
    if (counts[2] !== 0 || counts[0] !== 1 || counts[3] !== 1) throw new Error(`après suppression d'une occurrence (semaine +4) : ${counts.join(',')}`);
    log(`occurrence de la semaine +4 supprimée : ${counts.join(',')}`);

    // 6. Supprimer la série.
    await weekAhead(5);
    await selectChip(series);
    await deleteSelected(true);
    counts = await perWeek(series, 2, 5);
    if (counts.some((c) => c !== 0)) throw new Error(`après suppression de la série : ${counts.join(',')}, attendu 0,0,0,0,0`);
    log('série supprimée : plus aucune occurrence');
  } finally {
    await sweepAppointments();
    if (kind0) await setKind(kind0).catch(() => {});
  }
}
