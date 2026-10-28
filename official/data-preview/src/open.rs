//! Format detection and open: turn the raw bytes of a file into a paged data
//! source, dispatching on the file extension.
use crate::columnar;
use crate::sheet;
use crate::sqlite;
use crate::table;
use crate::tabular;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Mutex;

pub(crate) enum Source {
    Sqlite(rusqlite::Connection),
    /// Workbook bytes plus a per-sheet cache: sheets parse only when opened.
    Sheets {
        bytes: Vec<u8>,
        extension: String,
        cache: Mutex<BTreeMap<String, table::Table>>,
    },
    Single(table::Table),
}

pub(crate) struct Cache {
    pub(crate) path: String,
    pub(crate) source: Source,
    /// The `open` response, kept so `info.resolve` and `data.open` share one
    /// parse instead of parsing the same workbook twice.
    pub(crate) meta: Value,
}

fn extension(path: &str) -> String {
    path.rsplit_once('.')
        .map(|(_, extension)| extension.to_ascii_lowercase())
        .unwrap_or_default()
}

fn dataset(id: &str, name: &str, kind: &str) -> Value {
    json!({ "id": id, "name": name, "kind": kind })
}

pub(crate) fn page_of(source: &Source, dataset: &str, p: &Value) -> Result<Value, String> {
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
        Source::Sheets {
            bytes,
            extension,
            cache,
        } => {
            let mut sheets = cache.lock().map_err(|_| "sheet cache unavailable")?;
            if !sheets.contains_key(dataset) {
                let parsed = sheet::sheet(bytes, extension, dataset)?;
                sheets.insert(dataset.to_string(), parsed);
            }
            Ok(table::page(
                sheets.get(dataset).unwrap(),
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

pub(crate) fn open(path: &str, bytes: Vec<u8>) -> Result<(Value, Cache), String> {
    let extension = extension(path);
    match extension.as_str() {
        "db" | "sqlite" | "sqlite3" => {
            let connection = sqlite::open(bytes)?;
            let datasets = sqlite::datasets(&connection)?;
            if datasets.is_empty() {
                return Err(crate::i18n::t("数据库里没有表").into());
            }
            let summary = sqlite::summary(&connection)?;
            let cache = Cache {
                path: path.to_string(),
                source: Source::Sqlite(connection),
                meta: Value::Null,
            };
            Ok((
                json!({ "format": "sqlite", "datasets": datasets, "summary": summary }),
                cache,
            ))
        }
        "csv" | "tsv" => {
            let table = tabular::csv(bytes, path)?;
            let page = table::page(&table, 0, 100, None, false);
            let cache = Cache {
                path: path.to_string(),
                source: Source::Single(table),
                meta: Value::Null,
            };
            Ok((
                json!({
                    "format": extension,
                    "datasets": [dataset("data", crate::i18n::t("数据"), "table")],
                    "page": page,
                }),
                cache,
            ))
        }
        "xlsx" | "xls" => {
            let names = sheet::sheet_names(&bytes, &extension)?;
            let datasets = names
                .iter()
                .map(|name| dataset(name, name, "sheet"))
                .collect::<Vec<_>>();
            let first = sheet::sheet(&bytes, &extension, &names[0])?;
            let page = table::page(&first, 0, 100, None, false);
            let mut cache = BTreeMap::new();
            cache.insert(names[0].clone(), first);
            Ok((
                json!({
                    "format": extension,
                    "datasets": datasets,
                    "page": page,
                }),
                Cache {
                    path: path.to_string(),
                    source: Source::Sheets {
                        bytes,
                        extension,
                        cache: Mutex::new(cache),
                    },
                    meta: Value::Null,
                },
            ))
        }
        "parquet" => {
            let table = columnar::parquet(bytes)?;
            let page = table::page(&table, 0, 100, None, false);
            Ok((
                json!({ "format": "parquet", "datasets": [dataset("data", crate::i18n::t("数据"), "table")], "page": page }),
                Cache {
                    path: path.to_string(),
                    source: Source::Single(table),
                    meta: Value::Null,
                },
            ))
        }
        "arrow" | "feather" | "ipc" => {
            let table = columnar::arrow(bytes)?;
            let page = table::page(&table, 0, 100, None, false);
            Ok((
                json!({ "format": extension, "datasets": [dataset("data", crate::i18n::t("数据"), "table")], "page": page }),
                Cache {
                    path: path.to_string(),
                    source: Source::Single(table),
                    meta: Value::Null,
                },
            ))
        }
        "avro" => {
            let table = columnar::avro(bytes)?;
            let page = table::page(&table, 0, 100, None, false);
            Ok((
                json!({ "format": "avro", "datasets": [dataset("data", crate::i18n::t("数据"), "table")], "page": page }),
                Cache {
                    path: path.to_string(),
                    source: Source::Single(table),
                    meta: Value::Null,
                },
            ))
        }
        _ => Err(crate::i18n::t("暂不支持预览此格式").into()),
    }
}
