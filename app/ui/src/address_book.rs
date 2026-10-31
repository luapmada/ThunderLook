//! Autocomplétion des destinataires et boîte « Carnet d'adresses » (lot BC), utilisées par la rédaction.
//!
//! Les propositions viennent de la commande `address_suggest` (contacts, listes de distribution, adresses déjà vues dans
//! le courrier) ; rien n'est écrit dans Outlook ni dans les contacts.

use leptos::{html, prelude::*, task::spawn_local};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

/// Nombre de propositions sous le champ ; le Carnet en montre davantage.
const INLINE_LIMIT: u32 = 8;
const BOOK_LIMIT: u32 = 500;
/// Frappe → requête (ms) : évite une lecture du courrier envoyé à chaque touche.
const DEBOUNCE_MS: i32 = 150;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Sugg {
    /// `contact`, `list` ou `mail`.
    pub kind: String,
    pub name: String,
    pub address: String,
    pub insert: String,
    pub detail: String,
}

#[derive(Serialize)]
struct SuggestArgs<'a> {
    query: &'a str,
    limit: u32,
}

async fn suggest(query: &str, limit: u32) -> Result<Vec<Sugg>, String> {
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    let args = SuggestArgs { query, limit }.serialize(&ser).map_err(|e| e.to_string())?;
    let out = invoke("address_suggest", args).await.map_err(|e| e.as_string().unwrap_or_else(|| format!("address_suggest : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("address_suggest : {e}"))
}

pub(crate) async fn sleep(ms: i32) {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
}

fn kind_label(kind: &str) -> &'static str {
    match kind {
        "contact" => "Contact",
        "list" => "Liste",
        _ => "Courrier",
    }
}

/// Début du destinataire en cours de frappe : après le dernier « ; » ou « , » hors guillemets.
pub fn token_start(s: &str) -> usize {
    let (mut start, mut quoted) = (0, false);
    for (i, c) in s.char_indices() {
        match c {
            '"' => quoted = !quoted,
            ';' | ',' if !quoted => start = i + c.len_utf8(),
            _ => {}
        }
    }
    start
}

/// Remplace le destinataire en cours par `insert` et prépare le suivant (« ; » final).
pub fn complete(s: &str, insert: &str) -> String {
    let head = s[..token_start(s)].trim_end();
    if head.is_empty() { format!("{insert}; ") } else { format!("{head} {insert}; ") }
}

/// Ajoute `adds` à la fin d'une ligne de destinataires, sans doublon exact.
pub fn append(existing: &str, adds: &[String]) -> String {
    let mut out = existing.trim_end().trim_end_matches(';').trim_end().to_string();
    for a in adds {
        if out.contains(a.as_str()) {
            continue;
        }
        if !out.is_empty() {
            out.push_str("; ");
        }
        out.push_str(a);
    }
    out
}

const CSS: &str = "\
.addr-wrap{position:relative;display:block}.addr-wrap>input{width:100%;box-sizing:border-box}\
.addr-list{position:absolute;z-index:30;left:0;right:0;top:100%;margin:2px 0 0;padding:2px;list-style:none;max-height:260px;overflow:auto;\
background:var(--bg);color:var(--text);border:1px solid var(--border);border-radius:var(--radius);box-shadow:0 4px 14px rgba(0,0,0,.25)}\
.addr-list li{display:flex;gap:8px;align-items:baseline;padding:4px 8px;cursor:pointer;font-size:var(--fs)}\
.addr-list li[aria-selected=true],.addr-list li:hover{background:color-mix(in srgb,var(--accent) 22%,transparent)}\
.addr-list .a-name{font-weight:600}.addr-list .a-det,.addr-kind{color:var(--text-2);font-size:var(--fs-sm)}\
.addr-list .a-det{flex:1;min-width:0;overflow:hidden;text-overflow:ellipsis;white-space:nowrap}\
.addr-book{display:grid;gap:8px;min-width:min(760px,92vw)}\
.addr-book-cols{display:grid;grid-template-columns:3fr 1fr 2fr;gap:12px}\
.addr-book-results{margin:0;padding:0;list-style:none;overflow:auto;height:50vh;border:1px solid var(--border);border-radius:var(--radius)}\
.addr-book-results li{display:flex;gap:8px;align-items:center;padding:3px 8px}\
.addr-book-results li:hover{background:color-mix(in srgb,var(--accent) 14%,transparent)}\
.addr-book-results label{display:flex;gap:8px;align-items:baseline;flex:1;min-width:0;cursor:pointer}\
.addr-book-mid{display:flex;flex-direction:column;gap:8px;justify-content:center}\
.addr-book-dest{display:grid;gap:6px;align-content:start;overflow:auto;max-height:50vh}\
.addr-book-dest ul{margin:0;padding:0 0 0 14px;font-size:var(--fs-sm)}.addr-book-dest h3{margin:4px 0 0;font-size:var(--fs-sm)}";

/// Champ de destinataires avec liste de propositions (flèches, Entrée/Tab pour accepter, Échap pour fermer).
#[component]
pub fn AddressField(
    #[prop(into)] value: Signal<String>,
    on_set: Callback<String>,
    label: &'static str,
    #[prop(optional)] id: &'static str,
) -> impl IntoView {
    let items = RwSignal::new(Vec::<Sugg>::new());
    let active = RwSignal::new(0usize);
    let open = RwSignal::new(false);
    let generation = RwSignal::new(0u32);
    let list_id = format!("addr-list-{label}");

    let refresh = move |text: String| {
        let me = generation.get_untracked() + 1;
        generation.set(me);
        spawn_local(async move {
            sleep(DEBOUNCE_MS).await;
            if generation.get_untracked() != me {
                return;
            }
            let q = text[token_start(&text)..].trim().to_string();
            if q.chars().count() < 2 {
                open.set(false);
                items.set(Vec::new());
                return;
            }
            let found = suggest(&q, INLINE_LIMIT).await.unwrap_or_default();
            if generation.get_untracked() == me {
                active.set(0);
                open.set(!found.is_empty());
                items.set(found);
            }
        });
    };
    let accept = move |i: usize| {
        if let Some(s) = items.with_untracked(|v| v.get(i).cloned()) {
            let next = complete(&value.get_untracked(), &s.insert);
            on_set.run(next);
            generation.update(|g| *g += 1);
            open.set(false);
        }
    };
    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        if !open.get_untracked() {
            return;
        }
        let n = items.with_untracked(Vec::len);
        match ev.key().as_str() {
            "ArrowDown" if n > 0 => {
                ev.prevent_default();
                active.update(|a| *a = (*a + 1) % n);
            }
            "ArrowUp" if n > 0 => {
                ev.prevent_default();
                active.update(|a| *a = (*a + n - 1) % n);
            }
            "Enter" | "Tab" if n > 0 => {
                ev.prevent_default();
                accept(active.get_untracked());
            }
            "Escape" => {
                ev.prevent_default();
                ev.stop_propagation();
                open.set(false);
            }
            _ => {}
        }
    };

    view! {
        <div class="addr-wrap">
            <input
                id=id
                type="text"
                autocomplete="off"
                role="combobox"
                aria-label=label
                aria-autocomplete="list"
                aria-expanded=move || open.get().to_string()
                aria-controls=list_id.clone()
                prop:value=move || value.get()
                on:input=move |ev| {
                    let v = event_target_value(&ev);
                    on_set.run(v.clone());
                    refresh(v);
                }
                on:keydown=on_key
                on:blur=move |_| {
                    generation.update(|g| *g += 1);
                    open.set(false);
                }
            />
            <Show when=move || open.get()>
                <ul class="addr-list" id=list_id.clone() role="listbox" aria-label=format!("Propositions pour {label}")>
                    {move || {
                        items
                            .get()
                            .into_iter()
                            .enumerate()
                            .map(|(i, s)| {
                                view! {
                                    <li
                                        role="option"
                                        aria-selected=move || (active.get() == i).to_string()
                                        // `mousedown` + annulation : le champ garde le focus, donc pas de `blur` avant le choix.
                                        on:mousedown=move |ev| {
                                            ev.prevent_default();
                                            accept(i);
                                        }
                                        on:mousemove=move |_| active.set(i)
                                    >
                                        <span class="a-name">{s.name.clone()}</span>
                                        <span class="a-det">{s.detail.clone()}</span>
                                        <span class="addr-kind">{kind_label(&s.kind)}</span>
                                    </li>
                                }
                            })
                            .collect_view()
                    }}
                </ul>
            </Show>
        </div>
    }
}

/// Boîte « Carnet d'adresses » : recherche, cases à cocher, boutons À / Cc / Cci, puis OK.
/// `on_apply` reçoit les destinataires à ajouter (À, Cc, Cci).
#[component]
pub fn AddressBook(open: RwSignal<bool>, on_apply: Callback<(Vec<String>, Vec<String>, Vec<String>)>) -> impl IntoView {
    let query = RwSignal::new(String::new());
    let results = RwSignal::new(Vec::<Sugg>::new());
    let loading = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let checked = RwSignal::new(Vec::<String>::new());
    let (to, cc, bcc) = (RwSignal::new(Vec::<String>::new()), RwSignal::new(Vec::<String>::new()), RwSignal::new(Vec::<String>::new()));
    let generation = RwSignal::new(0u32);
    let search = NodeRef::<html::Input>::new();

    let load = move |q: String, delay: i32| {
        let me = generation.get_untracked() + 1;
        generation.set(me);
        loading.set(true);
        spawn_local(async move {
            if delay > 0 {
                sleep(delay).await;
            }
            if generation.get_untracked() != me {
                return;
            }
            let r = suggest(q.trim(), BOOK_LIMIT).await;
            if generation.get_untracked() == me {
                loading.set(false);
                match r {
                    Ok(v) => {
                        note.set(None);
                        results.set(v);
                    }
                    Err(e) => note.set(Some(e)),
                }
            }
        });
    };
    // À l'ouverture : listes vides, tout le carnet.
    Effect::new(move |_| {
        if open.get() {
            query.set(String::new());
            for l in [to, cc, bcc] {
                l.set(Vec::new());
            }
            checked.set(Vec::new());
            load(String::new(), 0);
            // `search.get()` suivi relançait l'effet au montage du champ (tout remis à zéro et carnet relu deux fois).
            request_animation_frame(move || {
                if let Some(i) = search.get_untracked() {
                    let _ = i.focus();
                }
            });
        }
    });
    let close = move || open.set(false);
    let move_to = move |dest: RwSignal<Vec<String>>| {
        let picked = checked.get_untracked();
        dest.update(|d| {
            for p in &picked {
                if !d.contains(p) {
                    d.push(p.clone());
                }
            }
        });
        checked.set(Vec::new());
    };
    let apply = move |_| {
        // Les cases cochées mais pas encore placées vont dans « À », comme un double-clic d'Outlook.
        let mut t = to.get_untracked();
        for p in checked.get_untracked() {
            if !t.contains(&p) {
                t.push(p);
            }
        }
        on_apply.run((t, cc.get_untracked(), bcc.get_untracked()));
        close();
    };
    let dest_block = move |title: &'static str, list: RwSignal<Vec<String>>| {
        view! {
            <h3>{title}</h3>
            <ul aria-label=title>
                {move || {
                    list.get()
                        .into_iter()
                        .map(|s| {
                            let s2 = s.clone();
                            view! {
                                <li>
                                    {s.clone()}
                                    " "
                                    <button class="link-btn" type="button" aria-label=format!("Retirer {s}") on:click=move |_| list.update(|l| l.retain(|x| *x != s2))>"×"</button>
                                </li>
                            }
                        })
                        .collect_view()
                }}
            </ul>
        }
    };

    view! {
        <style>{CSS}</style>
        <Show when=move || open.get()>
            <div class="modal-backdrop">
                <div
                    class="dialog wide addr-book"
                    role="dialog"
                    aria-modal="true"
                    aria-labelledby="addr-book-title"
                    on:keydown=move |ev| {
                        if ev.key() == "Escape" {
                            ev.prevent_default();
                            close();
                        }
                    }
                >
                    <h2 id="addr-book-title">"Carnet d'adresses"</h2>
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="alert">{n}</p> })}
                    <input
                        type="search"
                        node_ref=search
                        aria-label="Rechercher un nom ou une adresse"
                        placeholder="Rechercher un nom ou une adresse"
                        prop:value=move || query.get()
                        on:input=move |ev| {
                            let v = event_target_value(&ev);
                            query.set(v.clone());
                            load(v, DEBOUNCE_MS);
                        }
                    />
                    <div class="addr-book-cols">
                        <ul class="addr-book-results" aria-label="Résultats">
                            {move || {
                                if results.with(Vec::is_empty) {
                                    let t = if loading.get() { "Recherche…" } else { "Aucun résultat." };
                                    return view! { <li>{t}</li> }.into_any();
                                }
                                results
                                    .get()
                                    .into_iter()
                                    .map(|s| {
                                        let (a, b) = (s.insert.clone(), s.insert.clone());
                                        view! {
                                            <li>
                                                <label>
                                                    <input
                                                        type="checkbox"
                                                        prop:checked=move || checked.with(|c| c.contains(&a))
                                                        on:change=move |ev| {
                                                            let on = event_target_checked(&ev);
                                                            checked.update(|c| {
                                                                c.retain(|x| *x != b);
                                                                if on {
                                                                    c.push(b.clone());
                                                                }
                                                            });
                                                        }
                                                    />
                                                    <span class="a-name">{s.name.clone()}</span>
                                                    <span class="a-det">{s.detail.clone()}</span>
                                                </label>
                                                <span class="addr-kind">{kind_label(&s.kind)}</span>
                                            </li>
                                        }
                                    })
                                    .collect_view()
                                    .into_any()
                            }}
                        </ul>
                        <div class="addr-book-mid">
                            <button class="btn" type="button" prop:disabled=move || checked.with(Vec::is_empty) on:click=move |_| move_to(to)>"À →"</button>
                            <button class="btn" type="button" prop:disabled=move || checked.with(Vec::is_empty) on:click=move |_| move_to(cc)>"Cc →"</button>
                            <button class="btn" type="button" prop:disabled=move || checked.with(Vec::is_empty) on:click=move |_| move_to(bcc)>"Cci →"</button>
                        </div>
                        <div class="addr-book-dest">{dest_block("À", to)} {dest_block("Cc", cc)} {dest_block("Cci", bcc)}</div>
                    </div>
                    <div class="dialog-buttons">
                        <button class="btn primary" type="button" on:click=apply>"OK"</button>
                        <button class="btn" type="button" on:click=move |_| close()>"Annuler"</button>
                    </div>
                </div>
            </div>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jeton_en_cours() {
        assert_eq!(token_start("jean"), 0);
        assert_eq!(token_start("a@x.fr; ma"), 7);
        assert_eq!(token_start("\"Dupont, Jean\" <j@x.fr>, mar"), 24);
        assert_eq!(token_start("\"Dupont, Je"), 0);
    }

    #[test]
    fn completion_remplace_le_jeton() {
        assert_eq!(complete("mar", "Léa Martin <lea@x.fr>"), "Léa Martin <lea@x.fr>; ");
        assert_eq!(complete("a@x.fr; mar", "Léa <l@x.fr>"), "a@x.fr; Léa <l@x.fr>; ");
        assert_eq!(complete("a@x.fr;", "b@x.fr"), "a@x.fr; b@x.fr; ");
    }

    #[test]
    fn ajout_sans_doublon() {
        assert_eq!(append("", &["a@x.fr".into(), "b@x.fr".into()]), "a@x.fr; b@x.fr");
        assert_eq!(append("a@x.fr; ", &["a@x.fr".into(), "c@x.fr".into()]), "a@x.fr; c@x.fr");
    }
}
