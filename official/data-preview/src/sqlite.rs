//! SQLite datasets: tables/views, columns, indexes, foreign keys, paged rows.
use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};
use serde_json::{json, Value};
use std::io::Cursor;

const MAX_TEXT: usize = 10_000;

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
            let trimmed: String = value.chars().take(MAX_TEXT).collect();
            json!(trimmed)
        }
        ValueRef::Blob(bytes) => json!({ "blob": bytes.len() }),
    }
}

pub fn open(bytes: Vec<u8>) -> Result<Connection, String> {
    let mut connection = Connection::open_with_flags(
        ":memory:",
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE,
    )
    .map_err(|error| error.to_string())?;
    connection
        .deserialize_read_exact("main", Cursor::new(bytes.clone()), bytes.len(), true)
        .map_err(|error| format!("不是有效的 SQLite 文件: {error}"))?;
    Ok(connection)
}

pub fn datasets(connection: &Connection) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(
            "SELECT name, type FROM sqlite_master \
             WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' ORDER BY name",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(0)?,
                "kind": row.get::<_, String>(1)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn summary(connection: &Connection) -> Result<Value, String> {
    let datasets = datasets(connection)?;
    let Some(first) = datasets.first() else {
        return Ok(serde_json::json!({"datasets": 0, "columns": 0, "rows": 0}));
    };
    let name = first
        .get("name")
        .and_then(Value::as_str)
        .ok_or("dataset has no name")?;
    let columns = columns(connection, name)?;
    let rows: i64 = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", quote(name)),
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(serde_json::json!({
        "datasets": datasets.len(),
        "columns": columns.len(),
        "rows": rows,
    }))
}

fn columns(connection: &Connection, table: &str) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({})", quote(table)))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "name": row.get::<_, String>(1)?,
                "kind": row.get::<_, String>(2)?,
                "notNull": row.get::<_, i64>(3)? != 0,
                "default": row.get::<_, Option<String>>(4)?,
                "primaryKey": row.get::<_, i64>(5)? != 0,
            }))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn indexes(connection: &Connection, table: &str) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({})", quote(table)))
        .map_err(|error| error.to_string())?;
    let listed = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)? != 0,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(|error| error.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    let mut values = Vec::new();
    for (name, unique, origin) in listed {
        let mut info = connection
            .prepare(&format!("PRAGMA index_info({})", quote(&name)))
            .map_err(|error| error.to_string())?;
        let fields = info
            .query_map([], |row| row.get::<_, String>(2))
            .map_err(|error| error.to_string())?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        values.push(json!({ "name": name, "unique": unique, "origin": origin, "columns": fields }));
    }
    Ok(values)
}

fn foreign_keys(connection: &Connection, table: &str) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare(&format!("PRAGMA foreign_key_list({})", quote(table)))
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, i64>(0)?,
                "table": row.get::<_, String>(2)?,
                "from": row.get::<_, String>(3)?,
                "to": row.get::<_, Option<String>>(4)?,
            }))
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

pub fn page(
    connection: &Connection,
    table: &str,
    offset: u64,
    limit: u64,
    order: Option<&str>,
    descending: bool,
) -> Result<Value, String> {
    let known = connection
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type IN ('table','view') AND name = ?1",
            rusqlite::params![table],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
    if known == 0 {
        return Err("no such table".into());
    }
    let columns = columns(connection, table)?;
    let total = connection
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", quote(table)),
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|error| error.to_string())?;
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
        .map_err(|error| error.to_string())?;
    let column_count = statement.column_count();
    let mut rows = statement
        .query(rusqlite::params![limit as i64, offset as i64])
        .map_err(|error| error.to_string())?;
    let mut values = Vec::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let mut cells = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value = row.get_ref(index).map_err(|error| error.to_string())?;
            cells.push(text(value));
        }
        values.push(cells);
    }
    Ok(json!({
        "columns": columns,
        "rows": values,
        "total": total,
        "offset": offset,
        "limit": limit,
        "order": order,
        "descending": descending,
        "indexes": indexes(connection, table)?,
        "foreignKeys": foreign_keys(connection, table)?,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> Connection {
        let connection = Connection::open_in_memory().unwrap();
        connection
            .execute_batch(
                "CREATE TABLE authors (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
                 CREATE TABLE books (id INTEGER PRIMARY KEY, title TEXT, price REAL, author_id INTEGER REFERENCES authors(id));
                 CREATE INDEX books_author ON books(author_id);
                 INSERT INTO authors (name) VALUES ('Ada');
                 INSERT INTO books (title, price, author_id) VALUES ('Alpha', 9.5, 1), ('Beta', NULL, 1);",
            )
            .unwrap();
        connection
    }

    #[test]
    fn lists_datasets_columns_and_rows() {
        let connection = fixture();
        let datasets = datasets(&connection).unwrap();
        assert_eq!(datasets[0]["id"], "authors");
        let value = page(&connection, "books", 0, 100, Some("title"), true).unwrap();
        assert_eq!(value["total"], 2);
        assert_eq!(value["rows"][0][1], "Beta");
        assert_eq!(value["columns"][2]["kind"], "REAL");
        assert_eq!(value["indexes"][0]["name"], "books_author");
        assert_eq!(value["foreignKeys"][0]["table"], "authors");
    }

    #[test]
    fn opens_serialized_bytes_read_only() {
        let connection = fixture();
        let serialized = connection.serialize("main").unwrap();
        let reopened = open(serialized.to_vec()).unwrap();
        assert_eq!(datasets(&reopened).unwrap().len(), 2);
        assert!(reopened
            .execute("INSERT INTO authors (name) VALUES ('Eve')", [])
            .is_err());
    }
}
