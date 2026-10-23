//! Read-only SQLite browser: tables, columns, indexes, foreign keys and rows.
//!
//! The file bytes come through the host's brokered file API and are opened with
//! `sqlite3_deserialize` in read-only mode, so the database is never written to
//! disk by the plugin and the original file cannot be mutated.
use fiex_plugin_sdk::{self as sdk, RpcError};
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::cell::RefCell;
use std::io::Cursor;

const MAX_TEXT: usize = 10_000;
const MAX_LIMIT: u64 = 500;
const DEFAULT_LIMIT: u64 = 100;

struct Cache {
    path: String,
    connection: Connection,
}

fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

fn text(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(number) => json!(number),
        ValueRef::Real(number) => json!(number),
        ValueRef::Text(bytes) => {
            let value = String::from_utf8_lossy(bytes);
            let mut trimmed: String = value.chars().take(MAX_TEXT).collect();
            if value.chars().count() > MAX_TEXT {
                trimmed.push('…');
            }
            json!(trimmed)
        }
        ValueRef::Blob(bytes) => json!({ "blob": bytes.len() }),
    }
}

fn columns(connection: &Connection, table: &str) -> Result<Vec<Value>, RpcError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({})", quote(table)))
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "cid": row.get::<_, i64>(0)?,
                "name": row.get::<_, String>(1)?,
                "type": row.get::<_, String>(2)?,
                "notNull": row.get::<_, i64>(3)? != 0,
                "default": row.get::<_, Option<String>>(4)?,
                "primaryKey": row.get::<_, i64>(5)? != 0,
            }))
        })
        .map_err(|error| RpcError::internal(error.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| RpcError::internal(error.to_string()))
}

fn indexes(connection: &Connection, table: &str) -> Result<Vec<Value>, RpcError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({})", quote(table)))
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let listed = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| RpcError::internal(error.to_string()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let mut values = Vec::new();
    for (name, unique, origin) in listed {
        let mut info = connection
            .prepare(&format!("PRAGMA index_info({})", quote(&name)))
            .map_err(|error| RpcError::internal(error.to_string()))?;
        let fields = info
            .query_map([], |row| row.get::<_, String>(2))
            .map_err(|error| RpcError::internal(error.to_string()))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| RpcError::internal(error.to_string()))?;
        values.push(json!({
            "name": name,
            "unique": unique,
            "origin": origin,
            "columns": fields,
        }));
    }
    Ok(values)
}

fn foreign_keys(connection: &Connection, table: &str) -> Result<Vec<Value>, RpcError> {
    let mut statement = connection
        .prepare(&format!("PRAGMA foreign_key_list({})", quote(table)))
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "table": row.get::<_, String>(2)?,
                "from": row.get::<_, String>(3)?,
                "to": row.get::<_, Option<String>>(4)?,
            }))
        })
        .map_err(|error| RpcError::internal(error.to_string()))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| RpcError::internal(error.to_string()))
}

fn tables(connection: &Connection) -> Result<Value, RpcError> {
    let mut statement = connection
        .prepare(
            "SELECT name, type, sql FROM sqlite_master \
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "name": row.get::<_, String>(0)?,
                "type": row.get::<_, String>(1)?,
                "sql": row.get::<_, Option<String>>(2)?,
            }))
        })
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let entries = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| RpcError::internal(error.to_string()))?;
    Ok(json!({ "tables": entries }))
}

fn table_data(
    connection: &Connection,
    table: &str,
    columns: &[Value],
    limit: u64,
    offset: u64,
    order: Option<&str>,
    descending: bool,
) -> Result<Value, RpcError> {
    let count = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", quote(table)),
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let order_clause = order
        .filter(|name| columns.iter().any(|column| column["name"] == *name))
        .map(|name| {
            format!(
                " ORDER BY {} {}",
                quote(name),
                if descending { "DESC" } else { "ASC" }
            )
        })
        .unwrap_or_default();
    let sql = format!(
        "SELECT * FROM {}{} LIMIT ?1 OFFSET ?2",
        quote(table),
        order_clause
    );
    let mut statement = connection
        .prepare(&sql)
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let column_count = statement.column_count();
    let mut rows = statement
        .query(rusqlite::params![limit as i64, offset as i64])
        .map_err(|error| RpcError::internal(error.to_string()))?;
    let mut values = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|error| RpcError::internal(error.to_string()))?
    {
        let mut cells = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value = row
                .get_ref(index)
                .map_err(|error| RpcError::internal(error.to_string()))?;
            cells.push(text(value));
        }
        values.push(cells);
    }
    Ok(json!({
        "columns": columns,
        "rows": values,
        "total": count,
        "limit": limit,
        "offset": offset,
        "order": order,
        "descending": descending,
        "indexes": indexes(connection, table)?,
        "foreignKeys": foreign_keys(connection, table)?,
    }))
}

fn open(bytes: Vec<u8>) -> Result<Connection, RpcError> {
    let mut connection = Connection::open_with_flags(
        ":memory:",
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .map_err(|error| RpcError::internal(error.to_string()))?;
    connection
        .deserialize_read_exact("main", Cursor::new(bytes.clone()), bytes.len(), true)
        .map_err(|error| RpcError::internal(format!("不是有效的 SQLite 文件: {error}")))?;
    Ok(connection)
}

fn dispatch(
    method: &str,
    p: Value,
    cache: &RefCell<Option<Cache>>,
    host: &mut dyn sdk::HostServices,
) -> Result<Value, RpcError> {
    match method {
        "sqlite.open" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let bytes = host
                .files()
                .read(path)
                .map_err(|error| RpcError::internal(format!("读取文件失败: {error}")))?;
            let connection = open(bytes)?;
            let value = tables(&connection)?;
            *cache.borrow_mut() = Some(Cache {
                path: path.to_string(),
                connection,
            });
            Ok(value)
        }
        "sqlite.table" => {
            let path = p
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing path"))?;
            let table = p
                .get("table")
                .and_then(Value::as_str)
                .ok_or_else(|| RpcError::invalid_params("missing table"))?;
            {
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
                    *cached = Some(Cache {
                        path: path.to_string(),
                        connection: open(bytes)?,
                    });
                }
            }
            let cached = cache.borrow();
            let connection = &cached.as_ref().unwrap().connection;
            let known = connection
                .query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
                    rusqlite::params![table],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(|error| RpcError::internal(error.to_string()))?;
            if known == 0 {
                return Err(RpcError::invalid_params("no such table"));
            }
            let columns = columns(connection, table)?;
            let limit = p
                .get("limit")
                .and_then(Value::as_u64)
                .unwrap_or(DEFAULT_LIMIT)
                .clamp(1, MAX_LIMIT);
            let offset = p.get("offset").and_then(Value::as_u64).unwrap_or(0);
            let order = p.get("order").and_then(Value::as_str);
            let descending = p
                .get("descending")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            table_data(
                connection, table, &columns, limit, offset, order, descending,
            )
        }
        _ => Err(RpcError::method_not_found(method)),
    }
}

fn main() {
    let cache = RefCell::new(None);
    if let Err(error) =
        sdk::serve_with_host("org.fiex.sqlite-preview", move |method, params, host| {
            dispatch(method, params, &cache, host)
        })
    {
        eprintln!("SQLite preview IPC failed: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "PRAGMA foreign_keys = ON;
                 CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT NOT NULL, bio TEXT);
                 CREATE TABLE books (
                   id INTEGER PRIMARY KEY,
                   title TEXT NOT NULL,
                   price REAL,
                   cover BLOB,
                   author_id INTEGER REFERENCES authors(id)
                 );
                 CREATE INDEX books_author ON books(author_id);
                 INSERT INTO authors (name, bio) VALUES ('Ada', NULL), ('Bob', 'writer');
                 INSERT INTO books (title, price, cover, author_id)
                   VALUES ('Alpha', 9.5, x'0102', 1), ('Beta', NULL, NULL, 2);",
            )
            .unwrap();
        connection
    }

    #[test]
    fn lists_tables_and_views() {
        let connection = fixture();
        connection
            .execute_batch("CREATE VIEW priced AS SELECT * FROM books WHERE price IS NOT NULL")
            .unwrap();
        let value = tables(&connection).unwrap();
        let names = value["tables"]
            .as_array()
            .unwrap()
            .iter()
            .map(|table| table["name"].as_str().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["authors", "books", "priced"]);
        assert_eq!(value["tables"][2]["type"], "view");
    }

    #[test]
    fn describes_columns_indexes_and_foreign_keys() {
        let connection = fixture();
        let columns = columns(&connection, "books").unwrap();
        assert_eq!(columns.len(), 5);
        assert_eq!(columns[0]["name"], "id");
        assert_eq!(columns[0]["primaryKey"], true);
        assert_eq!(columns[1]["notNull"], true);
        let value = table_data(&connection, "books", &columns, 10, 0, None, false).unwrap();
        assert_eq!(value["indexes"][0]["name"], "books_author");
        assert_eq!(value["foreignKeys"][0]["table"], "authors");
    }

    #[test]
    fn pages_sorts_and_encodes_values() {
        let connection = fixture();
        let columns = columns(&connection, "books").unwrap();
        let value = table_data(&connection, "books", &columns, 1, 1, Some("title"), true).unwrap();
        assert_eq!(value["total"], 2);
        assert_eq!(value["rows"].as_array().unwrap().len(), 1);
        assert_eq!(value["rows"][0][1], "Alpha");
        let value = table_data(&connection, "books", &columns, 10, 0, None, false).unwrap();
        assert_eq!(value["rows"][0][2], 9.5);
        assert_eq!(value["rows"][1][2], Value::Null);
        assert_eq!(value["rows"][0][3]["blob"], 2);
    }

    #[test]
    fn opening_bytes_is_read_only() {
        let connection = fixture();
        let serialized = connection.serialize("main").unwrap();
        let reopened = open(serialized.to_vec()).unwrap();
        let value = tables(&reopened).unwrap();
        assert_eq!(value["tables"].as_array().unwrap().len(), 2);
        assert!(reopened
            .execute("INSERT INTO authors (name) VALUES ('Eve')", [])
            .is_err());
    }
}
