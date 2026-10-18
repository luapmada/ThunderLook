//! Appels au cœur Rust via l'IPC de Tauri (`window.__TAURI__.core.invoke`).

use serde::{Serialize, de::DeserializeOwned};
use thunderlook_model::{
    CategoryInfo, FolderOp, FolderTree, IndexStatus, ListQuery, ListView, MessageOp, MessageSummary, MessageView, OpResult,
    SearchQuery, Selection, SortField, settings::Settings,
};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], catch)]
    async fn invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}

async fn call<A: Serialize, T: DeserializeOwned>(cmd: &str, args: &A) -> Result<T, String> {
    // Les u64 (jetons de vue) passent en nombres JS : ils restent très en dessous de 2^53.
    let ser = serde_wasm_bindgen::Serializer::json_compatible();
    let args = args.serialize(&ser).map_err(|e| e.to_string())?;
    let out = invoke(cmd, args)
        .await
        .map_err(|e| e.as_string().unwrap_or_else(|| format!("{cmd} : {e:?}")))?;
    serde_wasm_bindgen::from_value(out).map_err(|e| format!("{cmd} : {e}"))
}

#[derive(Serialize)]
struct NoArgs {}

#[derive(Serialize)]
struct SettingsArg {
    settings: Settings,
}

#[derive(Serialize)]
struct QueryArg<'a> {
    query: &'a ListQuery,
}

#[derive(Serialize)]
struct RowsArg {
    token: u64,
    start: u32,
    count: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ViewArg<'a> {
    id: &'a str,
    allow_remote: bool,
}

#[derive(Serialize)]
struct AttachmentArg<'a> {
    id: &'a str,
    index: u32,
}

#[derive(Serialize)]
struct PerfArg<'a> {
    label: &'a str,
    ms: f64,
}

pub async fn folder_tree() -> Result<FolderTree, String> {
    call("folder_tree", &NoArgs {}).await
}

pub async fn open_list(query: &ListQuery) -> Result<ListView, String> {
    call("open_list", &QueryArg { query }).await
}

pub async fn list_rows(token: u64, start: u32, count: u32) -> Result<Vec<MessageSummary>, String> {
    call("list_rows", &RowsArg { token, start, count }).await
}

pub async fn message_view(id: &str, allow_remote: bool) -> Result<MessageView, String> {
    call("message_view", &ViewArg { id, allow_remote }).await
}

#[derive(Serialize)]
struct OpArg<'a> {
    selection: &'a Selection,
    op: &'a MessageOp,
}

#[derive(Serialize)]
struct FolderOpArg<'a> {
    op: &'a FolderOp,
}

pub async fn apply_op(selection: &Selection, op: &MessageOp) -> Result<OpResult, String> {
    call("apply_op", &OpArg { selection, op }).await
}

pub async fn folder_op(op: &FolderOp) -> Result<OpResult, String> {
    call("folder_op", &FolderOpArg { op }).await
}

#[derive(Serialize)]
struct SearchArg<'a> {
    query: &'a SearchQuery,
    sort: SortField,
    descending: bool,
}

pub async fn search(query: &SearchQuery, sort: SortField, descending: bool) -> Result<ListView, String> {
    call("search", &SearchArg { query, sort, descending }).await
}

pub async fn index_status() -> Result<IndexStatus, String> {
    call("index_status", &NoArgs {}).await
}

pub async fn undo() -> Result<Option<String>, String> {
    call("undo", &NoArgs {}).await
}

pub async fn categories() -> Result<Vec<CategoryInfo>, String> {
    call("categories", &NoArgs {}).await
}

pub async fn open_attachment(id: &str, index: u32) -> Result<(), String> {
    call("open_attachment", &AttachmentArg { id, index }).await
}

pub async fn save_attachment(id: &str, index: u32) -> Result<bool, String> {
    call("save_attachment", &AttachmentArg { id, index }).await
}

pub async fn load_settings() -> Result<Settings, String> {
    call("load_settings", &NoArgs {}).await
}

pub async fn save_settings(settings: Settings) -> Result<(), String> {
    call("save_settings", &SettingsArg { settings }).await
}

/// Mesure de performance, journalisée par le cœur si `THUNDERLOOK_PERF` est défini.
pub async fn perf_mark(label: &str, ms: f64) {
    let _ = call::<_, ()>("perf_mark", &PerfArg { label, ms }).await;
}
