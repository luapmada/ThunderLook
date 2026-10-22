//! Boîte « Paramètres du compte » (J7) : comptes de messagerie, ajout (connexion vérifiée avant l'enregistrement),
//! suppression (rien n'est effacé sur le serveur). Le mot de passe part au cœur, qui le range dans le gestionnaire
//! d'identifiants de Windows ; il n'est jamais gardé par l'interface.

use leptos::{html, prelude::*, task::spawn_local};
use thunderlook_model::AccountSummary;
use wasm_bindgen::JsCast;

use crate::{
    actions, ipc,
    state::{Dialog, use_ui},
};

#[component]
pub fn AccountsDialog() -> impl IntoView {
    let ui = use_ui();
    let root: NodeRef<html::Div> = NodeRef::new();
    let list = RwSignal::new(Vec::<AccountSummary>::new());
    let current = RwSignal::new(0usize);
    let adding = RwSignal::new(false);
    let busy = RwSignal::new(false);
    let note = RwSignal::new(None::<String>);
    let (address, name, password) = (RwSignal::new(String::new()), RwSignal::new(String::new()), RwSignal::new(String::new()));
    let (imap, smtp) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    // Port et chiffrement (« tls » : SSL/TLS dès l'ouverture, « starttls ») de chaque serveur, comme les « Paramètres
    // avancés » d'Outlook.
    let (imap_port, imap_sec) = (RwSignal::new(String::new()), RwSignal::new(String::new()));
    let (smtp_port, smtp_sec) = (RwSignal::new(String::new()), RwSignal::new(String::new()));

    let focus_first = move || {
        request_animation_frame(move || {
            if let Some(r) = root.get_untracked() {
                if let Ok(Some(el)) = r.query_selector("input, [data-autofocus], button") {
                    if let Ok(el) = el.dyn_into::<web_sys::HtmlElement>() {
                        let _ = el.focus();
                    }
                }
            }
        });
    };

    Effect::new(move |_| {
        if ui.dialog.get() != Some(Dialog::Accounts) {
            return;
        }
        adding.set(false);
        note.set(None);
        spawn_local(async move {
            match ipc::accounts().await {
                Ok(v) => list.set(v),
                Err(e) => note.set(Some(e)),
            }
            focus_first();
        });
    });

    let close = move || {
        password.set(String::new());
        ui.dialog.set(None);
        if let Some(l) = ui.refs.list.get_untracked() {
            let _ = l.focus();
        }
    };

    let start_add = move || {
        address.set(String::new());
        name.set(String::new());
        password.set(String::new());
        imap.set("imap.hostinger.com".into());
        smtp.set("smtp.hostinger.com".into());
        imap_port.set("993".into());
        imap_sec.set("tls".into());
        smtp_port.set("465".into());
        smtp_sec.set("tls".into());
        note.set(None);
        adding.set(true);
        focus_first();
    };

    let save = move || {
        if busy.get_untracked() {
            return;
        }
        let (a, n, p) = (address.get_untracked(), name.get_untracked(), password.get_untracked());
        if a.trim().is_empty() || p.is_empty() {
            note.set(Some("Indiquez l'adresse et le mot de passe.".into()));
            return;
        }
        let (i, s) = (imap.get_untracked(), smtp.get_untracked());
        let (Ok(ip), Ok(sp)) = (imap_port.get_untracked().trim().parse::<u16>(), smtp_port.get_untracked().trim().parse::<u16>()) else {
            note.set(Some("Indiquez un numéro de port valide pour chaque serveur.".into()));
            return;
        };
        let servers = ipc::AccountServers {
            imap_host: i,
            imap_port: ip,
            imap_security: imap_sec.get_untracked(),
            smtp_host: s,
            smtp_port: sp,
            smtp_security: smtp_sec.get_untracked(),
        };
        busy.set(true);
        note.set(Some("Vérification de la connexion…".into()));
        spawn_local(async move {
            match ipc::add_account(&a, &n, &p, &servers).await {
                Ok(v) => {
                    password.set(String::new());
                    list.set(v);
                    adding.set(false);
                    note.set(Some(format!("Compte {} ajouté ; première relève en cours.", a.trim())));
                    actions::refresh_tree(ui);
                }
                Err(e) => note.set(Some(e)),
            }
            busy.set(false);
        });
    };

    let remove = move || {
        let Some(acc) = list.with_untracked(|v| v.get(current.get_untracked()).cloned()) else { return };
        spawn_local(async move {
            match ipc::remove_account(&acc.id).await {
                Ok(v) => {
                    list.set(v);
                    current.set(0);
                    note.set(Some(format!("Compte {} retiré de ThunderLook (rien n'a été effacé sur le serveur).", acc.address)));
                    actions::refresh_tree(ui);
                    actions::reopen_view(ui);
                }
                Err(e) => note.set(Some(e)),
            }
        });
    };

    let on_key = move |ev: leptos::ev::KeyboardEvent| {
        match ev.key().as_str() {
            "Escape" => {
                if adding.get_untracked() && !list.with_untracked(Vec::is_empty) {
                    adding.set(false);
                } else {
                    close();
                }
            }
            "Enter" if adding.get_untracked() => save(),
            "ArrowDown" | "ArrowUp" if !adding.get_untracked() => {
                let n = list.with_untracked(Vec::len);
                if n > 0 {
                    let cur = current.get_untracked();
                    current.set(if ev.key() == "ArrowDown" { (cur + 1).min(n - 1) } else { cur.saturating_sub(1) });
                }
            }
            "Tab" => {
                if let Some(r) = root.get_untracked() {
                    let Ok(found) = r.query_selector_all("input, select, button, [tabindex='0']") else { return };
                    let els: Vec<web_sys::HtmlElement> = (0..found.length()).filter_map(|i| found.item(i)?.dyn_into().ok()).collect();
                    let active = document().active_element();
                    let pos = els.iter().position(|e| active.as_ref().is_some_and(|a| a == e.unchecked_ref::<web_sys::Element>()));
                    let n = els.len() as i32;
                    if n > 0 {
                        let next = (pos.map_or(0, |p| p as i32) + if ev.shift_key() { -1 } else { 1 }).rem_euclid(n);
                        let _ = els[next as usize].focus();
                    }
                }
            }
            _ => return,
        }
        ev.prevent_default();
        ev.stop_propagation();
    };

    let field = move |label: &'static str, kind: &'static str, value: RwSignal<String>, autocomplete: &'static str| {
        view! {
            <label class="field">
                <span>{label}</span>
                <input
                    type=kind
                    aria-label=label
                    autocomplete=autocomplete
                    prop:value=move || value.get()
                    on:input=move |ev| value.set(event_target_value(&ev))
                />
            </label>
        }
    };

    // Port et chiffrement d'un serveur ; changer de chiffrement change aussi le port s'il était celui par défaut de
    // l'autre (993 ↔ 143, 465 ↔ 587).
    let security_row = move |server: &'static str, port: RwSignal<String>, sec: RwSignal<String>, defaults: (&'static str, &'static str)| {
        let port_label = format!("Port {server}");
        let sec_label = format!("Chiffrement {server}");
        view! {
            <div class="field-row">
                <label class="field">
                    <span>"Port"</span>
                    <input
                        type="text"
                        inputmode="numeric"
                        aria-label=port_label
                        autocomplete="off"
                        prop:value=move || port.get()
                        on:input=move |ev| port.set(event_target_value(&ev))
                    />
                </label>
                <label class="field">
                    <span>"Chiffrement"</span>
                    <select
                        aria-label=sec_label
                        prop:value=move || sec.get()
                        on:change=move |ev| {
                            let next = event_target_value(&ev);
                            let (tls, starttls) = defaults;
                            port.update(|p| {
                                if next == "starttls" && p.trim() == tls {
                                    *p = starttls.into();
                                } else if next == "tls" && p.trim() == starttls {
                                    *p = tls.into();
                                }
                            });
                            sec.set(next);
                        }
                    >
                        <option value="tls">"SSL/TLS"</option>
                        <option value="starttls">"STARTTLS"</option>
                    </select>
                </label>
            </div>
        }
    };

    view! {
        <Show when=move || ui.dialog.get() == Some(Dialog::Accounts)>
            <div class="modal-backdrop" on:pointerdown=move |ev| {
                if ev.target() == ev.current_target() {
                    close();
                }
            }>
                <div class="dialog wide" role="dialog" aria-modal="true" aria-labelledby="acc-title" node_ref=root on:keydown=on_key>
                    <h2 id="acc-title">{move || if adding.get() { "Ajouter un compte" } else { "Paramètres du compte" }}</h2>
                    {move || if adding.get() {
                        view! {
                            {field("Adresse", "email", address, "username")}
                            {field("Nom", "text", name, "name")}
                            {field("Mot de passe", "password", password, "current-password")}
                            {field("Serveur IMAP", "text", imap, "off")}
                            {security_row("IMAP", imap_port, imap_sec, ("993", "143"))}
                            {field("Serveur SMTP", "text", smtp, "off")}
                            {security_row("SMTP", smtp_port, smtp_sec, ("465", "587"))}
                            <p class="dialog-note">"Hostinger : IMAP 993 et SMTP 465 en SSL/TLS (ou 143 et 587 en STARTTLS). Le mot de passe est gardé par Windows (Gestionnaire d'identification)."</p>
                        }.into_any()
                    } else {
                        view! {
                            <div class="rules-list" role="listbox" aria-label="Comptes" tabindex="0" data-autofocus
                                aria-activedescendant=move || format!("acc-{}", current.get())>
                                {move || list.get().into_iter().enumerate().map(|(i, a)| {
                                    let detail = format!(
                                        "IMAP {} · SMTP {} · {}{}",
                                        a.imap,
                                        a.smtp,
                                        match (&a.last_error, &a.last_sync) {
                                            (Some(e), _) => format!("erreur : {e}"),
                                            (None, Some(t)) => format!("relevé le {t}"),
                                            (None, None) => "jamais relevé".into(),
                                        },
                                        match a.pending {
                                            0 => String::new(),
                                            1 => " · 1 changement à reporter sur le serveur".into(),
                                            n => format!(" · {n} changements à reporter sur le serveur"),
                                        }
                                    );
                                    view! {
                                        <div role="option" id=format!("acc-{i}") class="rules-item acc-item" aria-selected=move || (current.get() == i).to_string()
                                            on:click=move |_| current.set(i)>
                                            <span class="rules-name">{format!("{} — {}", a.address, a.sender_name)}</span>
                                            <span class="dialog-note">{detail}</span>
                                        </div>
                                    }
                                }).collect_view()}
                            </div>
                            <label class="check">
                                <input
                                    type="checkbox"
                                    aria-label="Appliquer les règles à l'arrivée"
                                    prop:checked=move || list.with(|v| v.get(current.get()).is_some_and(|a| a.rules_on_arrival))
                                    on:change=move |ev| {
                                        let on = event_target_checked(&ev);
                                        let Some(acc) = list.with_untracked(|v| v.get(current.get_untracked()).cloned()) else { return };
                                        spawn_local(async move {
                                            match ipc::set_account_rules(&acc.id, on).await {
                                                Ok(v) => list.set(v),
                                                Err(e) => note.set(Some(e)),
                                            }
                                        });
                                    }
                                />
                                <span>"Appliquer les règles à l'arrivée des messages de ce compte"</span>
                            </label>
                            <p class="dialog-note">"Décoché par défaut : tant qu'Outlook relève aussi ce compte, laissez les règles tourner d'un seul côté."</p>
                            <div class="rules-toolbar">
                                <button class="btn" on:click=move |_| start_add()>"Ajouter…"</button>
                                <button class="btn" on:click=move |_| remove()>"Supprimer"</button>
                            </div>
                        }.into_any()
                    }}
                    {move || note.get().map(|n| view! { <p class="dialog-note" role="status">{n}</p> })}
                    <div class="dialog-buttons">
                        {move || if adding.get() {
                            view! {
                                <button class="btn primary" disabled=move || busy.get() on:click=move |_| save()>"Enregistrer"</button>
                                <button class="btn" on:click=move |_| {
                                    if list.with_untracked(Vec::is_empty) { close() } else { adding.set(false) }
                                }>"Annuler"</button>
                            }.into_any()
                        } else {
                            view! { <button class="btn primary" on:click=move |_| close()>"Fermer"</button> }.into_any()
                        }}
                    </div>
                </div>
            </div>
        </Show>
    }
}
