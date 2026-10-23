// Cas L-rappels : un rendez-vous dans 5 minutes avec rappel 15 minutes avant est déjà « dû » ; le compteur
// « Rappels : N » de la barre d'état augmente. Écrit dans la base : exige THUNDERLOOK_DATA.
import { js, until, sleep, requireTestData, tag, ymd, hm } from './lib.mjs';
import { openCalendar, setKind, createAppointment, sweepAppointments, currentKind } from './data.mjs';

export const name = 'L-rappels : rendez-vous dans 5 min, rappel 15 min, le compteur augmente';

const count = async () => {
  const text = await js(`document.querySelector('.rem-count')?.textContent ?? ''`);
  const m = text.match(/Rappels\s*:\s*(\d+)/);
  if (!m) throw new Error(`compteur « Rappels : N » introuvable dans la barre d'état (texte : « ${text} »)`);
  return +m[1];
};

export async function run(log) {
  if (!requireTestData(log)) return;
  const subject = tag('p');
  let kind0 = null;
  try {
    const before = await count();
    log(`compteur avant : ${before}`);
    const start = new Date(Date.now() + 5 * 60_000);
    const end = new Date(start.getTime() + 30 * 60_000);
    if (ymd(0, start) !== ymd(0, end)) throw new Error('le créneau passe minuit : relancer le cas dans quelques minutes');
    await openCalendar();
    kind0 = await currentKind();
    await setKind('Semaine');
    await createAppointment({ subject, date: ymd(0, start), from: hm(start), to: hm(end), reminder: '15' });
    log(`créé : « ${subject} » à ${hm(start)}, rappel 15 min avant`);
    // Le cœur signale les rappels échus à intervalle régulier : on laisse jusqu'à 2 minutes.
    await until(`Number((document.querySelector('.rem-count')?.textContent.match(/(\\d+)/) ?? [])[1]) > ${before}`, 'compteur de rappels en hausse', 120_000);
    log(`compteur après : ${await count()}`);
    await js(`document.querySelector('.rem-count').click()`);
    await until(`!!document.querySelector('.rem-panel')`, 'panneau des rappels');
    const inPanel = await js(`document.querySelector('.rem-panel').textContent.includes(${JSON.stringify(subject)})`);
    if (!inPanel) throw new Error(`« ${subject} » absent du panneau des rappels`);
    log('panneau des rappels : le rendez-vous y figure');
    await js(`document.querySelector('.rem-panel').dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`);
    await sleep(200);
  } finally {
    await sweepAppointments(1);
    if (kind0) await setKind(kind0).catch(() => {});
    await sleep(500);
    await js(`document.querySelector('.rem-panel')?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }))`).catch(() => {});
  }
}
