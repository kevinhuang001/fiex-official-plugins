//! Shared tabular model used by every non-SQLite format.
use serde::Serialize;
use serde_json::{json, Value};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Column {
    pub name: String,
    pub kind: String,
}

#[derive(Clone)]
pub struct Table {
    pub columns: Vec<Column>,
    pub rows: Vec<Vec<Value>>,
    pub note: Option<String>,
}

pub fn compare(left: &Value, right: &Value) -> std::cmp::Ordering {
    match (left, right) {
        (Value::Null, Value::Null) => std::cmp::Ordering::Equal,
        (Value::Null, _) => std::cmp::Ordering::Greater,
        (_, Value::Null) => std::cmp::Ordering::Less,
        (Value::Number(a), Value::Number(b)) => a
            .as_f64()
            .partial_cmp(&b.as_f64())
            .unwrap_or(std::cmp::Ordering::Equal),
        (Value::Bool(a), Value::Bool(b)) => a.cmp(b),
        (a, b) => a.to_string().cmp(&b.to_string()),
    }
}

pub fn page(
    table: &Table,
    offset: usize,
    limit: usize,
    order: Option<&str>,
    descending: bool,
) -> Value {
    let order_index =
        order.and_then(|name| table.columns.iter().position(|column| column.name == name));
    let mut indices: Vec<usize> = (0..table.rows.len()).collect();
    if let Some(index) = order_index {
        indices.sort_by(|a, b| compare(&table.rows[*a][index], &table.rows[*b][index]));
        if descending {
            indices.reverse();
        }
    }
    let rows = indices
        .iter()
        .skip(offset)
        .take(limit.min(500))
        .map(|index| table.rows[*index].clone())
        .collect::<Vec<_>>();
    json!({
        "columns": table.columns,
        "rows": rows,
        "total": table.rows.len(),
        "offset": offset,
        "limit": limit,
        "order": order,
        "descending": descending,
        "note": table.note,
    })
}

pub fn cell(value: &str) -> Value {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Value::String(String::new());
    }
    if let Ok(number) = trimmed.parse::<i64>() {
        return json!(number);
    }
    if let Ok(number) = trimmed.parse::<f64>() {
        if number.is_finite() {
            return json!(number);
        }
    }
    match trimmed {
        "true" | "TRUE" => return json!(true),
        "false" | "FALSE" => return json!(false),
        _ => {}
    }
    Value::String(trimmed.chars().take(10_000).collect())
}

pub fn kind_of(rows: &[Vec<Value>], index: usize) -> String {
    let mut number = true;
    let mut boolean = true;
    let mut any = false;
    for row in rows {
        match row.get(index) {
            Some(Value::Null) | None => continue,
            Some(Value::Number(_)) => boolean = false,
            Some(Value::Bool(_)) => number = false,
            Some(_) => {
                number = false;
                boolean = false;
            }
        }
        any = true;
    }
    if !any {
        return "text".into();
    }
    if number {
        "number".into()
    } else if boolean {
        "boolean".into()
    } else {
        "text".into()
    }
}
