//! Delimited text tables (CSV / TSV) with header detection and typed cells.
use crate::table::{cell, kind_of, Column, Table};

const MAX_ROWS: usize = 200_000;

fn delimiter(bytes: &[u8], path: &str) -> u8 {
    if path.to_lowercase().ends_with(".tsv") {
        return b'\t';
    }
    let first = bytes
        .split(|byte| *byte == b'\n')
        .next()
        .unwrap_or_default();
    let mut best = (b',', 0usize);
    for candidate in *b",;\t|" {
        let count = first.iter().filter(|byte| **byte == candidate).count();
        if count > best.1 {
            best = (candidate, count);
        }
    }
    best.0
}

pub fn csv(bytes: Vec<u8>, path: &str) -> Result<Table, String> {
    let delimiter = delimiter(&bytes, path);
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(bytes.as_slice());
    let mut records = reader.records();
    let header = records
        .next()
        .transpose()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "CSV 没有内容".to_string())?;
    let mut names: Vec<String> = Vec::new();
    for (index, value) in header.iter().enumerate() {
        let mut name = value.trim().to_string();
        if name.is_empty() {
            name = format!("列 {}", index + 1);
        }
        while names.contains(&name) {
            name.push('_');
        }
        names.push(name);
    }
    let width = names.len();
    let mut rows: Vec<Vec<serde_json::Value>> = Vec::new();
    for record in records.take(MAX_ROWS) {
        let record = record.map_err(|error| error.to_string())?;
        let mut row = (0..width)
            .map(|index| {
                record
                    .get(index)
                    .map(cell)
                    .unwrap_or(serde_json::Value::Null)
            })
            .collect::<Vec<_>>();
        if row
            .iter()
            .all(|value| value == &serde_json::Value::String(String::new()))
        {
            continue;
        }
        rows.push(std::mem::take(&mut row));
    }
    let columns = names
        .iter()
        .enumerate()
        .map(|(index, name)| Column {
            name: name.clone(),
            kind: kind_of(&rows, index),
        })
        .collect::<Vec<_>>();
    Ok(Table {
        columns,
        rows,
        note: Some(format!(
            "分隔符：{}",
            match delimiter {
                b'\t' => "Tab",
                b';' => "分号",
                b'|' => "竖线",
                _ => "逗号",
            }
        )),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_csv_with_types_and_duplicate_headers() {
        let table = csv(b"name,age,age\nAda,36,1\nGrace,85,2\n".to_vec(), "a.csv").unwrap();
        assert_eq!(table.columns.len(), 3);
        assert_eq!(table.columns[1].kind, "number");
        assert_eq!(table.rows[0][0], "Ada");
        assert_eq!(table.rows[1][1], 85);
    }

    #[test]
    fn parses_tsv_with_tab_delimiter() {
        let table = csv(b"a\tb\n1\t2\n".to_vec(), "a.tsv").unwrap();
        assert_eq!(table.columns.len(), 2);
        assert_eq!(table.rows[0][1], 2);
    }
}
