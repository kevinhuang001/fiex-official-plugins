//! Columnar / binary table formats: Parquet, Arrow IPC and Avro.
use crate::table::{kind_of, Column, Table};
use arrow::array::{ArrayRef, RecordBatch};
use arrow::datatypes::{DataType, Schema};
use arrow::util::display::array_value_to_string;
use serde_json::{json, Value};

const MAX_ROWS: usize = 200_000;

fn numeric(data_type: &DataType) -> bool {
    matches!(
        data_type,
        DataType::Int8
            | DataType::Int16
            | DataType::Int32
            | DataType::Int64
            | DataType::UInt8
            | DataType::UInt16
            | DataType::UInt32
            | DataType::UInt64
            | DataType::Float16
            | DataType::Float32
            | DataType::Float64
            | DataType::Decimal128(_, _)
            | DataType::Decimal256(_, _)
    )
}

fn cell(array: &ArrayRef, row: usize, data_type: &DataType) -> Value {
    let Ok(text) = array_value_to_string(array.as_ref(), row) else {
        return Value::Null;
    };
    if numeric(data_type) && !text.is_empty() {
        if let Ok(number) = text.parse::<i64>() {
            return json!(number);
        }
        if let Ok(number) = text.parse::<f64>() {
            return json!(number);
        }
    }
    if text.chars().count() > 10_000 {
        return json!(text.chars().take(10_000).collect::<String>());
    }
    json!(text)
}

fn from_batches(schema: &Schema, batches: Vec<RecordBatch>) -> Table {
    let columns = schema
        .fields()
        .iter()
        .map(|field| Column {
            name: field.name().clone(),
            kind: format!("{:?}", field.data_type()),
        })
        .collect::<Vec<_>>();
    let mut rows: Vec<Vec<Value>> = Vec::new();
    for batch in batches {
        for row in 0..batch.num_rows() {
            if rows.len() >= MAX_ROWS {
                break;
            }
            let mut cells = Vec::with_capacity(columns.len());
            for (index, field) in schema.fields().iter().enumerate() {
                cells.push(cell(batch.column(index), row, field.data_type()));
            }
            rows.push(cells);
        }
        if rows.len() >= MAX_ROWS {
            break;
        }
    }
    let columns = columns
        .into_iter()
        .enumerate()
        .map(|(index, column)| Column {
            name: column.name,
            kind: if column.kind == "Utf8" {
                kind_of(&rows, index)
            } else {
                column.kind
            },
        })
        .collect();
    Table {
        columns,
        rows,
        note: None,
    }
}

pub fn parquet(bytes: Vec<u8>) -> Result<Table, String> {
    use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
    let builder = ParquetRecordBatchReaderBuilder::try_new(bytes::Bytes::from(bytes))
        .map_err(|error| error.to_string())?;
    let schema = builder.schema().clone();
    let reader = builder.build().map_err(|error| error.to_string())?;
    let mut batches = Vec::new();
    for batch in reader {
        batches.push(batch.map_err(|error| error.to_string())?);
        if batches.len() > 1024 {
            break;
        }
    }
    Ok(from_batches(&schema, batches))
}

pub fn arrow(bytes: Vec<u8>) -> Result<Table, String> {
    use arrow::ipc::reader::{FileReader, StreamReader};
    use std::io::Cursor;
    if let Ok(reader) = FileReader::try_new(Cursor::new(bytes.clone()), None) {
        let schema = reader.schema();
        let batches = reader
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())?;
        return Ok(from_batches(&schema, batches));
    }
    let reader =
        StreamReader::try_new(Cursor::new(bytes), None).map_err(|error| error.to_string())?;
    let schema = reader.schema();
    let batches = reader
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    Ok(from_batches(&schema, batches))
}

fn avro_value(value: &apache_avro::types::Value) -> Value {
    use apache_avro::types::Value as Avro;
    match value {
        Avro::Null => Value::Null,
        Avro::Boolean(flag) => json!(flag),
        Avro::Int(number) => json!(number),
        Avro::Long(number) => json!(number),
        Avro::Float(number) => json!(number),
        Avro::Double(number) => json!(number),
        Avro::String(text) => json!(text.chars().take(10_000).collect::<String>()),
        Avro::Enum(_, symbol) => json!(symbol),
        Avro::Bytes(bytes) | Avro::Fixed(_, bytes) => json!({ "blob": bytes.len() }),
        Avro::Union(_, inner) => avro_value(inner),
        Avro::Array(items) => Value::Array(items.iter().map(avro_value).collect()),
        Avro::Map(map) => Value::Object(
            map.iter()
                .map(|(key, value)| (key.clone(), avro_value(value)))
                .collect(),
        ),
        Avro::Record(fields) => Value::Object(
            fields
                .iter()
                .map(|(key, value)| (key.clone(), avro_value(value)))
                .collect(),
        ),
        other => json!(format!("{other:?}")),
    }
}

pub fn avro(bytes: Vec<u8>) -> Result<Table, String> {
    let reader = apache_avro::Reader::new(bytes.as_slice()).map_err(|error| error.to_string())?;
    let schema = reader.writer_schema().clone();
    let fields = match &schema {
        apache_avro::Schema::Record(record) => record
            .fields
            .iter()
            .map(|field| field.name.clone())
            .collect::<Vec<_>>(),
        _ => vec!["value".into()],
    };
    let mut rows = Vec::new();
    for value in reader.take(MAX_ROWS) {
        let value = value.map_err(|error| error.to_string())?;
        let object = avro_value(&value);
        let row = fields
            .iter()
            .map(|field| match &object {
                Value::Object(map) => map.get(field).cloned().unwrap_or(Value::Null),
                _ => object.clone(),
            })
            .collect::<Vec<_>>();
        rows.push(row);
    }
    let columns = fields
        .into_iter()
        .enumerate()
        .map(|(index, name)| Column {
            name,
            kind: kind_of(&rows, index),
        })
        .collect();
    Ok(Table {
        columns,
        rows,
        note: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use arrow::array::{Int64Array, StringArray};
    use arrow::datatypes::{Field, Schema};
    use std::sync::Arc;

    fn batch() -> RecordBatch {
        let schema = Arc::new(Schema::new(vec![
            Field::new("id", DataType::Int64, false),
            Field::new("name", DataType::Utf8, true),
        ]));
        RecordBatch::try_new(
            schema,
            vec![
                Arc::new(Int64Array::from(vec![1, 2])),
                Arc::new(StringArray::from(vec![Some("Ada"), Some("Grace")])),
            ],
        )
        .unwrap()
    }

    #[test]
    fn reads_parquet_from_bytes() {
        let mut buffer = Vec::new();
        {
            use parquet::arrow::ArrowWriter;
            let mut writer = ArrowWriter::try_new(&mut buffer, batch().schema(), None).unwrap();
            writer.write(&batch()).unwrap();
            writer.close().unwrap();
        }
        let table = parquet(buffer).unwrap();
        assert_eq!(table.columns[0].name, "id");
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[0][1], "Ada");
    }

    #[test]
    fn reads_arrow_ipc_from_bytes() {
        let mut buffer = Vec::new();
        {
            use arrow::ipc::writer::FileWriter;
            let mut writer = FileWriter::try_new(&mut buffer, &batch().schema()).unwrap();
            writer.write(&batch()).unwrap();
            writer.finish().unwrap();
        }
        let table = arrow(buffer).unwrap();
        assert_eq!(table.rows.len(), 2);
        assert_eq!(table.rows[1][0], 2);
    }

    #[test]
    fn reads_avro_from_bytes() {
        use apache_avro::{Schema, Writer};
        let schema = Schema::parse_str(
            r#"{"type":"record","name":"row","fields":[{"name":"id","type":"long"},{"name":"name","type":"string"}]}"#,
        )
        .unwrap();
        let mut writer = Writer::new(&schema, Vec::new()).unwrap();
        writer
            .append_value(apache_avro::types::Value::Record(vec![
                ("id".into(), apache_avro::types::Value::Long(7)),
                (
                    "name".into(),
                    apache_avro::types::Value::String("Ada".into()),
                ),
            ]))
            .unwrap();
        let bytes = writer.into_inner().unwrap();
        let table = avro(bytes).unwrap();
        assert_eq!(table.rows[0][0], 7);
        assert_eq!(table.rows[0][1], "Ada");
    }
}
