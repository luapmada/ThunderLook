//! Séparateur redimensionnable (souris et clavier), role="separator".

use leptos::prelude::*;
use wasm_bindgen::JsCast;

#[component]
pub fn Splitter(
    label: &'static str,
    /// `true` : séparateur vertical entre deux colonnes (redimensionne une largeur).
    vertical: bool,
    #[prop(into)] value: Signal<u32>,
    min: u32,
    max: u32,
    on_change: Callback<u32>,
) -> impl IntoView {
    let drag = RwSignal::new(None::<(i32, u32)>); // (position de départ, valeur de départ)

    let pos = move |ev: &web_sys::PointerEvent| if vertical { ev.client_x() } else { ev.client_y() };
    let apply = move |v: i64| on_change.run(v.clamp(i64::from(min), i64::from(max)) as u32);

    view! {
        <div
            class="splitter"
            role="separator"
            tabindex="0"
            aria-label=label
            aria-orientation=if vertical { "vertical" } else { "horizontal" }
            aria-valuemin=min
            aria-valuemax=max
            aria-valuenow=move || value.get()
            on:pointerdown=move |ev| {
                if let Some(t) = ev.target().and_then(|t| t.dyn_into::<web_sys::Element>().ok()) {
                    let _ = t.set_pointer_capture(ev.pointer_id());
                }
                drag.set(Some((pos(&ev), value.get_untracked())));
            }
            on:pointermove=move |ev| {
                if let Some((start, from)) = drag.get_untracked() {
                    apply(i64::from(from) + i64::from(pos(&ev) - start));
                }
            }
            on:pointerup=move |_| drag.set(None)
            on:pointercancel=move |_| drag.set(None)
            on:keydown=move |ev| {
                let step = if ev.shift_key() { 64 } else { 16 };
                let delta = match (vertical, ev.key().as_str()) {
                    (true, "ArrowRight") | (false, "ArrowDown") => step,
                    (true, "ArrowLeft") | (false, "ArrowUp") => -step,
                    (_, "Home") => i64::from(min) - i64::from(value.get_untracked()),
                    (_, "End") => i64::from(max) - i64::from(value.get_untracked()),
                    _ => return,
                };
                ev.prevent_default();
                apply(i64::from(value.get_untracked()) + delta);
            }
        ></div>
    }
}
