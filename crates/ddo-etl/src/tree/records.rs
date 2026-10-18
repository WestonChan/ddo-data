pub(super) struct InsertedRecord {
    pub(super) row_count: usize,
    pub(super) row_id: i64,
}

macro_rules! database_record {
    (@mirror $name:ident $table:literal $array:literal) => {
        impl $name {
            pub(crate) const TABLE: &'static str = $table;
            pub(crate) const ARRAY: &'static str = $array;
        }
    };
    ($visibility:vis struct $name:ident in $table:literal as $array:literal { $($body:tt)* }) => {
        database_record!(@record [$table] [] $visibility struct $name { $($body)* });
        database_record!(@mirror $name $table $array);
    };
    ($(#[$attribute:meta])* $visibility:vis struct $name:ident in $table:literal as $array:literal { $($body:tt)* }) => {
        database_record!(@record [$table] [] $(#[$attribute])* #[schemars(extend("x-table" = $table, "x-array" = $array))] $visibility struct $name { $($body)* });
        database_record!(@mirror $name $table $array);
    };
    ($visibility:vis struct $name:ident in $table:literal { $($body:tt)* }) => {
        database_record!(@record [$table] [] $visibility struct $name { $($body)* });
    };
    ($(#[$attribute:meta])* $visibility:vis struct $name:ident in $table:literal { $($body:tt)* }) => {
        database_record!(@record [$table] [] $(#[$attribute])* #[schemars(extend("x-table" = $table))] $visibility struct $name { $($body)* });
    };
    ($(#[$attribute:meta])* $visibility:vis struct $name:ident in ($table:ident: &str $(, $parameter:ident: &str)*) { $($body:tt)* }) => {
        database_record!(@record [$table] [$table: &str, $($parameter: &str,)*] $(#[$attribute])* $visibility struct $name { $($body)* });
    };
    (@record [$table:expr] [$($parameters:tt)*]
        $(#[$attribute:meta])* $visibility:vis struct $name:ident {
            $(context { $($context:tt)* })?
            keys { $($keys:tt)* }
            $(values { $($values:tt)* })?
            fields { $($fields:tt)* }
            $($tail:tt)*
        }
    ) => {
        database_record!(@fields
            [[$table] [$($parameters)*] $(#[$attribute])* $visibility struct $name]
            [$(context { $($context)* })? keys { $($keys)* } $(values { $($values)* })?]
            [] [$($tail)*] [] $($fields)*);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*]
        [$(projected { $($projected:tt)* })? $(attached { $($attached:tt)* })? $(derived_corrections { $($derived:tt)* })?]
        [$($corrections:tt)*]) => {
        database_record!(@attached [$($header)*]
            [$($prefix)* fields { $($fields)* } $(projected { $($projected)* })?]
            [] [$(derived_corrections { $($derived)* })?] [$($corrections)*] $($($attached)*)?);
    };
    (@attached [$($header:tt)*] [$($prefix:tt)*] [$($attached:tt)*] [$($tail:tt)*] [$($corrections:tt)*]) => {
        database_record!(@emit $($header)* { $($prefix)* attached { $($attached)* } $($tail)* }
            corrections { $($corrections)* });
    };
    (@attached [$($header:tt)*] [$($prefix:tt)*] [$($attached:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty [correctable] $(, $($rest:tt)*)?) => {
        database_record!(@attached [$($header)*] [$($prefix)*]
            [$($attached)* $(#[$attribute])* $field: $field_type,] [$($tail)*]
            [$($corrections)* $field: $field_type => $field,] $($($rest)*)?);
    };
    (@attached [$($header:tt)*] [$($prefix:tt)*] [$($attached:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty $(, $($rest:tt)*)?) => {
        database_record!(@attached [$($header)*] [$($prefix)*]
            [$($attached)* $(#[$attribute])* $field: $field_type,] [$($tail)*]
            [$($corrections)*] $($($rest)*)?);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty => $column:ident [correctable] $([$($adapter:tt)*])? $(, $($rest:tt)*)?
    ) => {
        database_record!(@fields [$($header)*] [$($prefix)*] [$($fields)*] [$($tail)*] [$($corrections)*]
            $(#[$attribute])* $field: $field_type => $column [correctable $field] $([$($adapter)*])? $(, $($rest)*)?);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty => $column:ident [correctable $correction_name:ident] $([$($adapter:tt)*])? $(, $($rest:tt)*)?
    ) => {
        database_record!(@fields [$($header)*] [$($prefix)*] [$($fields)*] [$($tail)*]
            [$($corrections)* $correction_name: $field_type => $column $([$($adapter)*])?,]
            $(#[$attribute])* $field: $field_type => $column $([$($adapter)*])? $(, $($rest)*)?);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty => $column:ident [image $kind:literal] $(, $($rest:tt)*)?
    ) => {
        database_record!(@fields [$($header)*] [$($prefix)*]
            [$($fields)* $(#[$attribute])* #[schemars(extend("x-image-kind" = $kind))] $field: $field_type => $column [image $kind],]
            [$($tail)*] [$($corrections)*] $($($rest)*)?);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty => $column:ident [reference $target:literal $($label:literal)?] $(, $($rest:tt)*)?
    ) => {
        database_record!(@fields [$($header)*] [$($prefix)*]
            [$($fields)* $(#[$attribute])* #[schemars(extend("x-reference-kind" = $target))] $field: $field_type => $column [reference $target $($label)?],]
            [$($tail)*] [$($corrections)*] $($($rest)*)?);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty => $column:ident [optional_reference $target:literal $($label:literal)?] $(, $($rest:tt)*)?
    ) => {
        database_record!(@fields [$($header)*] [$($prefix)*]
            [$($fields)* $(#[$attribute])* #[schemars(extend("x-reference-kind" = $target))] $field: $field_type => $column [optional_reference $target $($label)?],]
            [$($tail)*] [$($corrections)*] $($($rest)*)?);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty => $column:ident [resolved_reference $target:literal $identity:ident] $(, $($rest:tt)*)?
    ) => {
        database_record!(@fields [$($header)*] [$($prefix)*]
            [$($fields)* $(#[$attribute])* #[schemars(extend("x-reference-kind" = $target))] $field: $field_type => $column [resolved_reference $target $identity],]
            [$($tail)*] [$($corrections)*] $($($rest)*)?);
    };
    (@fields [$($header:tt)*] [$($prefix:tt)*] [$($fields:tt)*] [$($tail:tt)*] [$($corrections:tt)*]
        $(#[$attribute:meta])* $field:ident: $field_type:ty => $column:ident $([$($adapter:tt)*])? $(, $($rest:tt)*)?
    ) => {
        database_record!(@fields [$($header)*] [$($prefix)*]
            [$($fields)* $(#[$attribute])* $field: $field_type => $column $([$($adapter)*])?,]
            [$($tail)*] [$($corrections)*] $($($rest)*)?);
    };
    (@emit [$table:expr] [$($table_parameter:tt)*]
        $(#[$attribute:meta])*
        $visibility:vis struct $name:ident {
            $(context { $($context:ident: $context_type:ty),* $(,)? })?
            keys { $($key:ident: $key_type:ty $(=> $key_column:expr)?),* $(,)? }
            $(values { $($value_column:ident => $value:expr),* $(,)? })?
            fields {
                $( $(#[$field_attribute:meta])* $field:ident: $field_type:ty => $column:ident $( [ $($adapter:tt)* ] )? ),* $(,)?
            }
            $(projected {
                $( $(#[$projected_attribute:meta])* $projected:ident: $projected_type:ty => $projection:expr ),* $(,)?
            })?
            $(attached {
                $( $(#[$attached_attribute:meta])* $attached:ident: $attached_type:ty ),* $(,)?
            })?
            $(derived_corrections { $($derived_name:ident { $($derived_field:ident => $derived_column:ident [$shape:ident $nullable:literal]),* $(,)? })* })?
        } corrections { $($correction_fields:tt)* }
    ) => {
        $(#[$attribute])*
        $visibility struct $name {
            $( $(#[$field_attribute])* $visibility $field: $field_type, )*
            $($( $(#[$projected_attribute])* $visibility $projected: $projected_type, )*)?
            $($( $(#[$attached_attribute])* $visibility $attached: $attached_type, )*)?
        }

        impl crate::tree::icon_references::IconReferences for $name {
            fn append_icons<'a>(&'a self, kind: &'a str, icons: &mut Vec<(&'a str, &'a str, &'static str)>) {
                let _ = (kind, &icons);
                $(database_record!(@icons self, $field, kind, icons $([$($adapter)*])?);)*
                $($(database_record!(@icons self, $attached, kind, icons);)*)?
            }
        }

        impl $name {
            database_record!(@correctable_declaration [$($correction_fields)*]);
            $($(pub(crate) const $derived_name: &'static [crate::corrections::CorrectableField] = &[
                $(crate::corrections::CorrectableField {
                    name: stringify!($derived_field),
                    column: stringify!($derived_column),
                    shape: crate::corrections::FieldShape::$shape,
                    is_nullable: $nullable,
                },)*
            ];)*)?

            database_record!(@citation_attachment [$table] [$($table_parameter)*] [$($($attached,)*)?]);

            pub(in crate::tree) fn select_columns($($table_parameter)*) -> String {
                let _ = $table;
                [$(format!("{} AS {}", database_record!(@key $key $(=> $key_column)?), stringify!($key)),)* $($(stringify!($value_column).to_string(),)*)? $(database_record!(@select $table, $column $( [ $($adapter)* ] )?),)* $($(format!("{} AS {}", $projection, stringify!($projected)),)*)?].join(", ")
            }

            pub(in crate::tree) fn from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<Self> {
                Ok(Self { $($field: database_record!(@read row, $column $( [ $($adapter)* ] )?)?,)* $($($projected: row.get(stringify!($projected))?,)*)? $($($attached: Default::default(),)*)? })
            }

            pub(in crate::tree) fn insert(
                &self, db: &rusqlite::Connection, $($table_parameter)* $($($context: $context_type,)*)? $($key: $key_type,)*
            ) -> anyhow::Result<crate::tree::records::InsertedRecord> {
                let columns = [$(database_record!(@key $key $(=> $key_column)?),)* $($(stringify!($value_column),)*)? $(stringify!($column),)*].map(|column| format!("[{column}]")).join(", ");
                let parameters = [$(concat!(":", stringify!($key)),)* $($(concat!(":", stringify!($value_column)),)*)? $(concat!(":", stringify!($column)),)*].join(", ");
                let inserted = db.prepare_cached(&format!("INSERT INTO {} ({columns}) VALUES ({parameters})", $table))?.execute(&[
                    $((concat!(":", stringify!($key)), &$key as &dyn rusqlite::ToSql),)*
                    $($((concat!(":", stringify!($value_column)), &$value as &dyn rusqlite::ToSql),)*)?
                    $((concat!(":", stringify!($column)), &database_record!(@write db, self, $field $( [ $($adapter)* ] )?) as &dyn rusqlite::ToSql),)*
                ][..])?;
                let row_id = db.last_insert_rowid();
                database_record!(@insert_annotations self, db, $table, [$($key,)*], [$($($attached,)*)?]);
                Ok(crate::tree::records::InsertedRecord { row_count: inserted, row_id })
            }
        }
    };
    (@correctable_declaration []) => {};
    (@correctable_declaration [$($field:ident: $field_type:ty => $column:ident $([$($adapter:tt)*])?),+ $(,)?]) => {
        pub(crate) const CORRECTABLE_FIELDS: &'static [crate::corrections::CorrectableField] = &[
            $(crate::corrections::CorrectableField {
                name: stringify!($field),
                column: stringify!($column),
                shape: database_record!(@correction_shape $field: $field_type $([$($adapter)*])?),
                is_nullable: <$field_type as crate::corrections::CorrectionFieldType>::IS_NULLABLE,
            },)*
        ];
    };
    (@correction_shape name: $field_type:ty $([$($adapter:tt)*])?) => { crate::corrections::FieldShape::RowName };
    (@correction_shape $field:ident: $field_type:ty [reference $target:literal $($label:literal)?]) => {
        crate::corrections::FieldShape::NamedReference { referenced_table: $target }
    };
    (@correction_shape $field:ident: $field_type:ty [optional_reference $target:literal $($label:literal)?]) => {
        crate::corrections::FieldShape::NamedReference { referenced_table: $target }
    };
    (@correction_shape $field:ident: $field_type:ty $([$($adapter:tt)*])?) => {
        <$field_type as crate::corrections::CorrectionFieldType>::SHAPE
    };
    (@citation_attachment [$table:expr] [$($parameters:tt)*] [citation, $($rest:ident,)*]) => {
        pub(in crate::tree) fn attach_citations(&mut self, annotations: &crate::tree::annotations::EntityAnnotations, $($parameters)* id: i64) {
            self.citation = annotations.entity($table, id).map(|annotation| annotation.citation.clone()).unwrap_or_default();
        }
    };
    (@citation_attachment [$table:expr] [$($parameters:tt)*] [$other:ident, $($rest:ident,)*]) => {
        database_record!(@citation_attachment [$table] [$($parameters)*] [$($rest,)*]);
    };
    (@citation_attachment [$table:expr] [$($parameters:tt)*] []) => {};
    (@insert_annotations $record:ident, $db:ident, $table:expr, [$($keys:ident,)*], [citation, $($rest:ident,)*]) => {
        database_record!(@citation_owner $record, $db, $table, [$(($keys $keys),)*]);
    };
    (@insert_annotations $record:ident, $db:ident, $table:expr, [$($keys:ident,)*], [$other:ident, $($rest:ident,)*]) => {
        database_record!(@insert_annotations $record, $db, $table, [$($keys,)*], [$($rest,)*]);
    };
    (@insert_annotations $record:ident, $db:ident, $table:expr, [$($keys:ident,)*], []) => {};
    (@citation_owner $record:ident, $db:ident, $table:expr, [(id $id:ident), $($rest:tt)*]) => {
        crate::tree::annotations::insert_citations($db, $table, Option::<i64>::from($id).unwrap_or_else(|| $db.last_insert_rowid()), &$record.citation)?;
    };
    (@citation_owner $record:ident, $db:ident, $table:expr, [($key:ident $binding:ident), $($rest:tt)*]) => {
        database_record!(@citation_owner $record, $db, $table, [$($rest)*]);
    };
    (@citation_owner $record:ident, $db:ident, $table:expr, []) => {
        compile_error!("a record with citations must have an id key");
    };
    (@icons $record:ident, $field:ident, $kind:ident, $icons:ident [image $image_kind:literal]) => {
        crate::tree::icon_references::ImageReference::append_image(&$record.$field, if $image_kind.is_empty() { $kind } else { $image_kind }, stringify!($field), $icons)
    };
    (@icons $record:ident, stance, $kind:ident, $icons:ident) => { crate::tree::icon_references::IconReferences::append_icons(&$record.stance, "stances", $icons) };
    (@icons $record:ident, option, $kind:ident, $icons:ident) => { crate::tree::icon_references::SocketOptionIcons::append_option_icons(&$record.option, $kind, $icons) };
    (@icons $record:ident, socket, $kind:ident, $icons:ident) => { crate::tree::icon_references::IconReferences::append_icons(&$record.socket, $kind, $icons) };
    (@icons $record:ident, enhancement, $kind:ident, $icons:ident) => { crate::tree::icon_references::IconReferences::append_icons(&$record.enhancement, $kind, $icons) };
    (@icons $record:ident, selection, $kind:ident, $icons:ident) => { crate::tree::icon_references::IconReferences::append_icons(&$record.selection, $kind, $icons) };
    (@icons $record:ident, sub_item, $kind:ident, $icons:ident) => { crate::tree::icon_references::IconReferences::append_icons(&$record.sub_item, $kind, $icons) };
    (@icons $record:ident, dc, $kind:ident, $icons:ident) => { crate::tree::icon_references::IconReferences::append_icons(&$record.dc, $kind, $icons) };
    (@icons $record:ident, attack, $kind:ident, $icons:ident) => { crate::tree::icon_references::IconReferences::append_icons(&$record.attack, $kind, $icons) };
    (@icons $record:ident, $field:ident, $kind:ident, $icons:ident $([$($adapter:tt)*])?) => {};
    (@key $key:ident => $column:expr) => { $column };
    (@key $key:ident) => { stringify!($key) };
    (@select $table:expr, $column:ident [reference $target:literal $label:literal]) => {
        format!("(SELECT {} FROM {} WHERE id = {}.{}) AS {}", $label, $target, $table, stringify!($column), stringify!($column))
    };
    (@select $table:expr, $column:ident [optional_reference $target:literal $label:literal]) => {
        database_record!(@select $table, $column [reference $target $label])
    };
    (@write $db:ident, $record:ident, $field:ident [reference $target:literal $label:literal]) => {
        $db.query_row(&format!("SELECT id FROM {} WHERE {} = ?1", $target, $label), [&$record.$field], |row| row.get::<_, i64>(0))?
    };
    (@write $db:ident, $record:ident, $field:ident [optional_reference $target:literal $label:literal]) => {
        $record.$field.as_deref().map(|label| $db.query_row(&format!("SELECT id FROM {} WHERE {} = ?1", $target, $label), [label], |row| row.get::<_, i64>(0))).transpose()?
    };
    (@select $table:expr, $column:ident [reference $target:literal]) => {
        format!("(SELECT name FROM {} WHERE id = {}.{}) AS {}", $target, $table, stringify!($column), stringify!($column))
    };
    (@select $table:expr, $column:ident [optional_reference $target:literal]) => {
        database_record!(@select $table, $column [reference $target])
    };
    (@select $table:expr, $column:ident [resolved_reference $target:literal $identity:ident]) => { database_record!(@select $table, $column [reference $target]) };
    (@select $table:expr, $column:ident $([$($adapter:tt)*])?) => {{ database_record!(@adapter $([$($adapter)*])?); format!("[{}]", stringify!($column)) }};
    (@read $row:ident, $column:ident [wiki_only]) => {
        $row.get::<_, String>(stringify!($column)).map(|provenance| provenance == "wiki")
    };
    (@write $db:ident, $record:ident, $field:ident [wiki_only]) => {
        if $record.$field { "wiki" } else { "maetrim" }
    };
    (@read $row:ident, $column:ident [enum_text]) => {
        $row.get::<_, String>(stringify!($column)).and_then(|text| serde_json::from_value(serde_json::Value::String(text)).map_err(|error| rusqlite::Error::FromSqlConversionFailure($row.as_ref().column_index(stringify!($column)).unwrap(), rusqlite::types::Type::Text, Box::new(error))))
    };
    (@write $db:ident, $record:ident, $field:ident [enum_text]) => {
        serde_json::to_value(&$record.$field)?.as_str().ok_or_else(|| anyhow::anyhow!("enum must serialize as text"))?.to_string()
    };
    (@read $row:ident, $column:ident [wiki $kind:literal]) => {
        $row.get::<_, String>(stringify!($column)).and_then(|url| {
            let name: String = $row.get("name")?;
            Ok((crate::tree::wiki_url_for_name($kind, &name).as_deref() != Some(url.as_str())).then_some(url))
        })
    };
    (@write $db:ident, $record:ident, $field:ident [wiki $kind:literal]) => {
        $record.$field.clone().or_else(|| crate::tree::wiki_url_for_name($kind, &$record.name)).ok_or_else(|| anyhow::anyhow!("{} {:?} has no wiki URL", $kind, $record.name))?
    };
    (@read $row:ident, $column:ident [integer_text]) => {
        $row.get::<_, Option<String>>(stringify!($column)).and_then(|value| value.map(|value| value.parse::<i64>().map_err(|error| rusqlite::Error::FromSqlConversionFailure($row.as_ref().column_index(stringify!($column)).unwrap(), rusqlite::types::Type::Text, Box::new(error)))).transpose())
    };
    (@write $db:ident, $record:ident, $field:ident [integer_text]) => { $record.$field.map(|number| number.to_string()) };
    (@write $db:ident, $record:ident, $field:ident [resolved_reference $target:literal $identity:ident]) => { $identity };
    (@write $db:ident, $record:ident, $field:ident [fallback $fallback:ident]) => { $record.$field.as_deref().unwrap_or($fallback) };
    (@write $db:ident, $record:ident, $field:ident [trimmed]) => { $record.$field.as_deref().map(str::trim) };
    (@write $db:ident, $record:ident, $field:ident [bonus]) => { $record.bonus.as_ref().or($record.bonus_type.as_ref()) };
    (@read $row:ident, $column:ident [json_required]) => { database_record!(@read $row, $column [json]) };
    (@write $db:ident, $record:ident, $field:ident [json_required]) => { serde_json::to_string(&$record.$field)? };
    (@write $db:ident, $record:ident, $field:ident [citation_page]) => {
        $record.$field.as_ref().or_else(|| $record.citation.iter().find(|citation| citation.field.is_empty()).map(|citation| &citation.url)).ok_or_else(|| anyhow::anyhow!("{} has no page or primary citation", $record.name))?
    };
    (@read $row:ident, $column:ident [optional_json_value]) => {
        $row.get::<_, Option<String>>(stringify!($column)).and_then(|value| value.map(|encoded| serde_json::from_str(&encoded)).transpose().map_err(|error| rusqlite::Error::FromSqlConversionFailure($row.as_ref().column_index(stringify!($column)).unwrap(), rusqlite::types::Type::Text, Box::new(error))))
    };
    (@write $db:ident, $record:ident, $field:ident [optional_json_value]) => { $record.$field.as_ref().map(serde_json::to_string).transpose()? };
    (@read $row:ident, $column:ident [json]) => { crate::tree::tables::json_list($row, $row.as_ref().column_index(stringify!($column))?) };
    (@read $row:ident, $column:ident [default_text]) => { $row.get::<_, Option<String>>(stringify!($column)).map(Option::unwrap_or_default) };
    (@read $row:ident, $column:ident $([$($adapter:tt)*])?) => {{ database_record!(@adapter $([$($adapter)*])?); $row.get(stringify!($column)) }};
    (@write $db:ident, $record:ident, $field:ident [reference $target:literal]) => { crate::tree::id_for_name($db, $target, &$record.$field)? };
    (@write $db:ident, $record:ident, $field:ident [optional_reference $target:literal]) => { $record.$field.as_deref().map(|name| crate::tree::id_for_name($db, $target, name)).transpose()? };
    (@write $db:ident, $record:ident, $field:ident [json]) => { crate::tree::optional_json(&$record.$field)? };
    (@write $db:ident, $record:ident, $field:ident $([$($adapter:tt)*])?) => {{ database_record!(@adapter $([$($adapter)*])?); &$record.$field }};

    (@adapter) => {};
    (@adapter [reference $target:literal $($label:literal)?]) => {};
    (@adapter [optional_reference $target:literal $($label:literal)?]) => {};
    (@adapter [resolved_reference $target:literal $identity:ident]) => {};
    (@adapter [enum_text ]) => {};
    (@adapter [wiki_only]) => {};
    (@adapter [wiki $kind:literal]) => {};
    (@adapter [integer_text ]) => {};
    (@adapter [fallback $fallback:ident]) => {};
    (@adapter [trimmed ]) => {};
    (@adapter [bonus ]) => {};
    (@adapter [optional_json_value ]) => {};
    (@adapter [json_required ]) => {};
    (@adapter [citation_page ]) => {};
    (@adapter [json ]) => {};
    (@adapter [default_text ]) => {};
    (@adapter [image $kind:literal]) => {};
    (@adapter [$adapter:ident $($arguments:tt)*]) => { compile_error!(concat!("unknown database adapter: ", stringify!($adapter))); };
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    database_record! {
        #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct TestRecord in "test_records" {
            keys { id: i64 }
            fields {
                name: String => name,
                amount: Option<i64> => amount,
                enabled: bool => enabled,
            }
            attached {
                #[serde(default, skip_serializing_if = "Vec::is_empty")]
                citation: Vec<crate::tree::Citation>,
            }
        }
    }

    database_record! {
        #[derive(Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
        #[serde(deny_unknown_fields)]
        pub(crate) struct TestSkill in (table: &str, owner_column: &str) {
            keys { owner_id: i64 => owner_column }
            fields { skill: String => skill }
        }
    }

    #[test]
    fn unknown_adapters_fail_during_macro_expansion() {
        use std::io::Write;
        use std::process::{Command, Stdio};
        let output_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/adapter-compile-test");
        std::fs::create_dir_all(&output_dir).unwrap();
        let source = format!("include!({:?}); fn main() {{ let _ = database_record!(@select \"items\", effect_id [refrence \"effects\"]); }}", std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/tree/records.rs"));
        let mut compiler = Command::new("rustc")
            .args(["--edition=2021", "--crate-name", "unknown_adapter", "--out-dir"])
            .arg(&output_dir)
            .arg("-")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        compiler.stdin.take().unwrap().write_all(source.as_bytes()).unwrap();
        let output = compiler.wait_with_output().unwrap();
        let errors = String::from_utf8(output.stderr).unwrap();
        assert!(!output.status.success(), "misspelled adapter compiled");
        assert!(errors.contains("unknown database adapter: refrence"), "{errors}");
    }

    #[test]
    fn shared_declaration_maps_owner_columns_in_both_directions() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE class_skills (class_id INTEGER, skill TEXT); CREATE TABLE race_skills (skill TEXT, race_id INTEGER)").unwrap();
        for (table, owner_column) in [("class_skills", "class_id"), ("race_skills", "race_id")] {
            let record = TestSkill { skill: "Balance".into() };
            record.insert(&db, table, owner_column, 7).unwrap();
            let (owner_id, restored) = db
                .query_row(
                    &format!("SELECT {} FROM {table}", TestSkill::select_columns(table, owner_column)),
                    [],
                    |row| Ok((row.get::<_, i64>("owner_id")?, TestSkill::from_row(row)?)),
                )
                .unwrap();
            assert_eq!(owner_id, 7);
            assert_eq!(restored.skill, "Balance");
        }
    }

    #[test]
    fn declared_citations_are_inserted_with_the_record() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(ddo_model::ddl()).unwrap();
        db.execute_batch("CREATE TABLE test_records (id INTEGER, name TEXT, amount INTEGER, enabled INTEGER)").unwrap();
        let record: TestRecord = toml::from_str("name = 'Reviewed'\nenabled = true\n[[citation]]\nfield = 'name'\nurl = 'https://ddowiki.com/page/Reviewed'\nread = '2026-10-18'\nnote = 'Checked the name.'\n").unwrap();
        record.insert(&db, 7).unwrap();
        let count: i64 = db.query_row("SELECT COUNT(*) FROM citations WHERE owner_kind = 'test_records' AND owner_id = 7 AND note = 'Checked the name.'", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 1);
        let mut restored = db
            .query_row(&format!("SELECT {} FROM test_records", TestRecord::select_columns()), [], TestRecord::from_row)
            .unwrap();
        restored.attach_citations(&crate::tree::annotations::EntityAnnotations::from_database(&db).unwrap(), 7);
        assert_eq!(toml::to_string(&restored).unwrap(), toml::to_string(&record).unwrap());
    }

    #[test]
    fn one_declaration_drives_column_order_in_both_directions() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE test_records (enabled INTEGER, amount INTEGER, name TEXT, id INTEGER)").unwrap();
        for (id, amount) in [(1, None), (2, Some(0)), (3, Some(i64::MAX))] {
            let record = TestRecord { name: format!("Record {id}"), amount, enabled: id == 3, citation: Vec::new() };
            record.insert(&db, id).unwrap();
            let restored = db
                .query_row(
                    &format!("SELECT {} FROM test_records WHERE id = ?1", TestRecord::select_columns()),
                    [id],
                    TestRecord::from_row,
                )
                .unwrap();
            assert_eq!(serde_json::to_value(&restored).unwrap(), serde_json::to_value(&record).unwrap());
        }
        let record = TestRecord { name: "Full".to_string(), amount: Some(5), enabled: true, citation: Vec::new() };
        assert_eq!(toml::to_string(&record).unwrap(), "name = \"Full\"\namount = 5\nenabled = true\n");
    }
}
