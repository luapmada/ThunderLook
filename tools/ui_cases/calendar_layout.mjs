// Cas L-calendrier : (1) vue Mois, aucun rendez-vous plus bas que 16 px (régression du 23/10) ;
// (2) vue Semaine, deux rendez-vous simultanés ne se recouvrent pas. Écrit dans la base : exige THUNDERLOOK_DATA.
import { js, until, sleep, requireTestData, tag, ymd } from './lib.mjs';
import { openCalendar, setKind, weekAhead, chips, createAppointment, sweepAppointments, currentKind } from './data.mjs';

export const name = 'L-calendrier : hauteur ≥ 16 px en vue Mois, pas de recouvrement en vue Semaine';

export async function run(log) {
  if (!requireTestData(log)) return;
  const a = tag('a');
  const b = tag('b');
  let kind0 = null;
  try {
    await openCalendar();
    kind0 = await currentKind();
    await setKind('Semaine');
    const date = ymd(14);
    await createAppointment({ subject: a, date, from: '09:00', to: '10:00' });
    await createAppointment({ subject: b, date, from: '09:00', to: '10:00' });

    // Semaine : deux colonnes côte à côte.
    await weekAhead(2);
    await until(`document.querySelectorAll('.cal-ev').length > 0`, 'rendez-vous de la semaine');
    const rects = await js(`JSON.stringify([${JSON.stringify(a)}, ${JSON.stringify(b)}].map(t => {
      const chip = [...document.querySelectorAll('.cal-ev')].find(e => e.getAttribute('aria-label').startsWith(t));
      const r = (chip?.closest('.cal-timed') ?? chip)?.getBoundingClientRect();
      return r ? { left: r.left, right: r.right, top: r.top, bottom: r.bottom } : null;
    }))`).then(JSON.parse);
    if (rects.includes(null)) throw new Error(`rendez-vous simultanés introuvables dans la semaine (${(await chips(a)).length}, ${(await chips(b)).length})`);
    const [r1, r2] = rects;
    const overlapX = Math.min(r1.right, r2.right) - Math.max(r1.left, r2.left);
    const overlapY = Math.min(r1.bottom, r2.bottom) - Math.max(r1.top, r2.top);
    if (r1.right - r1.left < 20 || r2.right - r2.left < 20) throw new Error(`largeur trop faible : ${r1.right - r1.left}, ${r2.right - r2.left} px`);
    if (overlapX > 1 && overlapY > 1) throw new Error(`recouvrement : ${overlapX.toFixed(1)} px en largeur, ${overlapY.toFixed(1)} px en hauteur`);
    log(`semaine : deux rendez-vous simultanés côte à côte (${(r1.right - r1.left).toFixed(0)} et ${(r2.right - r2.left).toFixed(0)} px de large), sans recouvrement`);

    // Mois : le mois affiché contient les deux rendez-vous ; aucun ne fait moins de 16 px de haut.
    await setKind('Mois');
    await until(`document.querySelectorAll('.cal-month .cal-ev').length > 0`, 'rendez-vous de la vue Mois', 10000);
    await sleep(300);
    const heights = await js(`JSON.stringify([...document.querySelectorAll('.cal-month .cal-ev')].map(e => ({ label: e.getAttribute('aria-label').slice(0, 40), h: e.getBoundingClientRect().height })))`).then(JSON.parse);
    const small = heights.filter((x) => x.h < 16);
    if (small.length) throw new Error(`${small.length} rendez-vous de moins de 16 px en vue Mois : ${small.slice(0, 3).map((x) => `« ${x.label} » ${x.h.toFixed(1)} px`).join(' ; ')}`);
    const min = Math.min(...heights.map((x) => x.h));
    log(`mois : ${heights.length} rendez-vous affichés, le plus bas fait ${min.toFixed(1)} px (≥ 16)`);
  } finally {
    await sweepAppointments(3);
    if (kind0) await setKind(kind0).catch(() => {});
  }
}
