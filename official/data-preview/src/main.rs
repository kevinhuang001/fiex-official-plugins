//! Table-structured data preview: SQLite, CSV/TSV, XLSX/XLS, Parquet, Arrow
//! IPC and Avro. Everything is paged; binary formats are parsed with their
//! format crates, never by hand.
mod columnar;
mod sheet;
mod sqlite;
mod table;
mod tabular;

use fiex_plugin_sdk as sdk;
use fiex_plugin_sdk::RpcError;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::BTreeMap;

enum Source {
    Sqlite(rusqlite::Connection),
    Sheets(BTreeMap<String, table::Table>),
    Single(table::Table),
}

struct Cache {
    path: String,
    source: Source,
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn dataset(id: &str, name: &str, kind: &str) -> Value {
    json!({ "id": id, "name": name, "kind": kind })
}

fn page_of(source: &Source, dataset: &str, p: &Value) -> Result<Value, String> {
    let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0);
    let limit = p
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(100)
        .clamp(1, 500);
    let order = p.get("order").and_then(Value::as_str);
    let descending = p
        .get("descending")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    match source {
        Source::Sqlite(connection) => {
            sqlite::page(connection, dataset, offset, limit, order, descending)
        }
        Source::Sheets(sheets) => {
            let sheet = sheets
                .get(dataset)
                .ok_or_else(|| "没有这个工作表".to_string())?;
            Ok(table::page(
                sheet,
                offset as usize,
                limit as usize,
                order,
                descending,
            ))
        }
        Source::Single(table) => Ok(table::page(
            table,
            offset as usize,
            limit as usize,
            order,
            descending,
        )),
    }
}

fn open(path: &str, bytes: Vec<u8>) -> Result<(Value, Cache), String> {
    let extension = extension(path);
    match extension.as_str() {
        "db" | "sqlite" | "sqlite3" => {
            let connection = sqlite::open(bytes)?;
            let datasets = sqlite::datasets(&connection)?;
            if datasets.is_empty() {
                return Err("数据库里没有表".into());
            }
            let cache = Cache {
                path: path.to_string(),
                source: Source::Sqlite(connection),
            };
            Ok((json!({ "format": "sqlite", "datasets": datasets }), cache))
        }
        "csv" | "tsv" => {
            let table = tabular::csv(bytes, path)?;
            let page = table::page(&table, 0, 100, None, false);
            let cache = Cache {
                path: path.to_string(),
                source: Source::Single(table),
            };
            Ok((
                json!({
                    "format": extension,
                    "datasets": [dataset("data", "数据", "table")],
                    "page": page,
                }),
                cache,
            ))
        }
        "xlsx" | "xls" => {
            let sheets = sheet::open(bytes, &extension)?;
            let datasets = sheets
                .keys()
                .map(|name| dataset(name, name, "sheet"))
                .collect::<Vec<_>>();
            let first = sheets.keys().next().cloned().unwrap_or_default();
            let page = table::page(&sheets[&first], 0, 100, None, false);
            Ok((
                json!({
                    "format": extension,
                    "datasets": datasets,
                    "page": page,
                }),
                Cache {
                    path: path.to_string(),
                    source: Source::Sheets(sheets),
                },
            ))
        }
        "parquet" => {
            let table = columnar::parquet(bytes)?;
            let page = table::page(&table, 0, 100, None, false);
            Ok((
                json!({ "format": "parquet", "datasets": [dataset("data", "数据", "table")], "page": page }),
                Cache {
                    path: path.to_string(),
                    source: Source::Single(table),
                },
            ))
        }
        "arrow" | "feather" | "ipc" => {
            let table = columnar::arrow(bytes)?;
            let page = table::page(&table, 0, 100, None, false);
            Ok((
                json!({ "format": extension, "datasets": [dataset("data", "数据", "table")], "page": page }),
                Cache {
                    path: path.to_string(),
                    source: Source::Single(table),
                },
            ))
        }
        "avro" => {
            let table = columnar::avro(bytes)?;
            let page = table::page(&table, 0, 100, None, false);
            Ok((
                json!({ "format": "avro", "datasets": [dataset("data", "数据", "table")], "page": page }),
                Cache {
                    path: path.to_string(),
                    source: Source::Single(table),
                },
            ))
        }
        _ => Err("暂不支持预览此格式".into()),
    }
}

fn dispatch(
    method: &str,
    p: Value,
    cache: &RefCell<Option<Cache>>,
    host: &mut dyn sdk::HostServices,
) -> Result<Value, RpcError> {
    match method {
        "data.open" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = host
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            let (value, opened) = open(path, bytes).map_err(RpcError::internal)?;
            *cache.borrow_mut() = Some(opened);
            Ok(value)
        }
        "data.page" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let dataset = p.get("dataset").and_then(Value::as_str).unwrap_or("data");
            let mut cached = cache.borrow_mut();
            if cached
                .as_ref()
                .map(|entry| entry.path != path)
                .unwrap_or(true)
            {
                let bytes = host
                    .files()
                    .read(path)
                    .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
                let (_, opened) = open(path, bytes).map_err(RpcError::internal)?;
                *cached = Some(opened);
            }
            let entry = cached.as_ref().unwrap();
            page_of(&entry.source, dataset, &p).map_err(RpcError::internal)
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    let cache = RefCell::new(None);
    if let Err(error) =
        sdk::serve_with_host("org.fiex.data-preview", move |method, params, host| {
            dispatch(method, params, &cache, host)
        })
    {
        eprintln!("Data preview IPC failed: {error}");
        std::process::exit(1);
    }
}
