//! Spreadsheet tables (XLSX/XLS) via calamine: one dataset per sheet.
use crate::table::{kind_of, Column, Table};
use calamine::{Data, Reader, Xls, Xlsx};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::io::Cursor;

const MAX_ROWS: usize = 200_000;

fn value(data: &Data) -> Value {
    match data {
        Data::Empty => Value::Null,
        Data::String(text) => json!(text.chars().take(10_000).collect::<String>()),
        Data::Float(number) => json!(number),
        Data::Int(number) => json!(number),
        Data::Bool(flag) => json!(flag),
        Data::DateTime(date) => json!(date.to_string()),
        Data::DateTimeIso(text) | Data::DurationIso(text) => json!(text),
        Data::Error(error) => json!(format!("#ERR: {error:?}")),
    }
}

fn sheet_table(range: &calamine::Range<Data>) -> Table {
    let mut rows_data = range.rows();
    let header = rows_data.next().map(|row| row.to_vec()).unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for (index, cell) in header.iter().enumerate() {
        let mut name = cell.to_string().trim().to_string();
        if name.is_empty() {
            name = format!("列 {}", index + 1);
        }
        while names.contains(&name) {
            name.push('_');
        }
        names.push(name);
    }
    let width = names.len();
    let mut rows = Vec::new();
    for row in rows_data.take(MAX_ROWS) {
        let mut cells = (0..width)
            .map(|index| row.get(index).map(value).unwrap_or(Value::Null))
            .collect::<Vec<_>>();
        if cells.iter().all(|cell| cell.is_null()) {
            continue;
        }
        cells.truncate(width);
        rows.push(std::mem::take(&mut cells));
    }
    let columns = names
        .into_iter()
        .enumerate()
        .map(|(index, name)| Column {
            name,
            kind: kind_of(&rows, index),
        })
        .collect::<Vec<_>>();
    Table {
        columns,
        rows,
        note: None,
    }
}

pub fn open(bytes: Vec<u8>, extension: &str) -> Result<BTreeMap<String, Table>, String> {
    let mut sheets = BTreeMap::new();
    if extension == "xls" {
        let mut workbook = Xls::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
        for name in workbook.sheet_names().to_vec() {
            if let Ok(range) = workbook.worksheet_range(&name) {
                sheets.insert(name, sheet_table(&range));
            }
        }
    } else {
        let mut workbook = Xlsx::new(Cursor::new(bytes)).map_err(|error| error.to_string())?;
        for name in workbook.sheet_names().to_vec() {
            if let Ok(range) = workbook.worksheet_range(&name) {
                sheets.insert(name, sheet_table(&range));
            }
        }
    }
    if sheets.is_empty() {
        return Err("工作簿没有可见的工作表".into());
    }
    Ok(sheets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_calamine_values() {
        assert_eq!(value(&Data::Float(1.5)), json!(1.5));
        assert_eq!(value(&Data::String("x".into())), json!("x"));
        assert_eq!(value(&Data::Empty), Value::Null);
    }
}

#[cfg(test)]
mod fixture_tests {
    use super::*;

    #[test]
    fn reads_the_committed_workbook() {
        let bytes = std::fs::read("tests/fixtures/sample.xlsx").unwrap();
        let sheets = open(bytes, "xlsx").unwrap();
        assert!(sheets.len() >= 2, "{:?}", sheets.keys());
        let budget = sheets.get("预算").expect("预算 sheet");
        assert!(budget.rows.len() >= 3);
        assert_eq!(budget.columns[0].name, "项目");
        assert_eq!(budget.rows[0][0], "研发");
    }
}
