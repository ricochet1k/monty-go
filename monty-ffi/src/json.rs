use monty::{DictPairs, ExcType, MontyObject};
use num_bigint::BigInt;
use serde_json::{json, Map, Value};

use crate::error::{FfiError, FfiResult};

const TUPLE_TAG: &str = "$tuple";
const BYTES_TAG: &str = "$bytes";
const SET_TAG: &str = "$set";
const FROZEN_SET_TAG: &str = "$frozenset";
const DICT_TAG: &str = "$dict";
const EXCEPTION_TAG: &str = "$exception";
const REPR_TAG: &str = "$repr";
const PATH_TAG: &str = "$path";
const BIGINT_TAG: &str = "$bigint";
const DATACLASS_TAG: &str = "$dataclass";
const NAMED_TUPLE_TAG: &str = "$named_tuple";

pub fn decode_inputs(json: &str) -> FfiResult<Vec<MontyObject>> {
    if json.trim().is_empty() {
        return Ok(Vec::new());
    }
    let value: Value = serde_json::from_str(json)?;
    match value {
        Value::Array(items) => items.into_iter().map(value_to_object).collect(),
        other => Err(FfiError::Message(format!(
            "expected JSON array for inputs, got {other}"
        ))),
    }
}

pub fn decode_object(json: &str) -> FfiResult<MontyObject> {
    let value: Value = serde_json::from_str(json)?;
    value_to_object(value)
}

pub fn encode_object(value: &MontyObject) -> FfiResult<String> {
    let json_value = object_to_value(value)?;
    serde_json::to_string(&json_value).map_err(Into::into)
}

pub fn encode_objects(values: &[MontyObject]) -> FfiResult<String> {
    let json_values: FfiResult<Vec<_>> = values.iter().map(object_to_value).collect();
    serde_json::to_string(&json_values?).map_err(Into::into)
}

pub fn encode_kwargs(values: &[(MontyObject, MontyObject)]) -> FfiResult<String> {
    let mut encoded = Vec::with_capacity(values.len());
    for (key, value) in values {
        encoded.push(Value::Array(vec![
            object_to_value(key)?,
            object_to_value(value)?,
        ]));
    }
    serde_json::to_string(&encoded).map_err(Into::into)
}

pub fn encode_u32_slice(values: &[u32]) -> FfiResult<String> {
    serde_json::to_string(values).map_err(Into::into)
}

fn value_to_object(value: Value) -> FfiResult<MontyObject> {
    match value {
        Value::Null => Ok(MontyObject::None),
        Value::Bool(b) => Ok(MontyObject::Bool(b)),
        Value::Number(num) => number_to_object(num),
        Value::String(s) => Ok(MontyObject::String(s)),
        Value::Array(items) => {
            let list: FfiResult<Vec<_>> = items.into_iter().map(value_to_object).collect();
            Ok(MontyObject::List(list?))
        }
        Value::Object(map) => object_map_to_object(map),
    }
}

fn number_to_object(num: serde_json::Number) -> FfiResult<MontyObject> {
    if let Some(i) = num.as_i64() {
        Ok(MontyObject::Int(i))
    } else if let Some(f) = num.as_f64() {
        Ok(MontyObject::Float(f))
    } else if let Some(u) = num.as_u64() {
        Ok(MontyObject::BigInt(BigInt::from(u)))
    } else {
        Err(FfiError::Message("invalid JSON number".into()))
    }
}

fn object_map_to_object(mut map: Map<String, Value>) -> FfiResult<MontyObject> {
    if let Some(tuple) = map.remove(TUPLE_TAG) {
        return match tuple {
            Value::Array(items) => {
                let converted: FfiResult<Vec<_>> = items.into_iter().map(value_to_object).collect();
                Ok(MontyObject::Tuple(converted?))
            }
            _ => Err(FfiError::Message("$tuple must be an array".into())),
        };
    }
    if let Some(bytes) = map.remove(BYTES_TAG) {
        return match bytes {
            Value::Array(items) => {
                let mut buffer = Vec::with_capacity(items.len());
                for value in items {
                    let number = value
                        .as_u64()
                        .ok_or_else(|| FfiError::Message("$bytes expects integers".into()))?;
                    buffer.push(number as u8);
                }
                Ok(MontyObject::Bytes(buffer))
            }
            _ => Err(FfiError::Message("$bytes must be an array".into())),
        };
    }
    if let Some(set_values) = map.remove(SET_TAG) {
        return parse_collection(set_values).map(MontyObject::Set);
    }
    if let Some(set_values) = map.remove(FROZEN_SET_TAG) {
        return parse_collection(set_values).map(MontyObject::FrozenSet);
    }
    if let Some(dict_values) = map.remove(DICT_TAG) {
        return parse_dict(dict_values).map(MontyObject::Dict);
    }
    if let Some(token) = map.remove(BIGINT_TAG) {
        return match token {
            Value::String(raw) => raw
                .parse::<BigInt>()
                .map(MontyObject::BigInt)
                .map_err(|err| FfiError::Message(format!("invalid bigint literal: {err}"))),
            _ => Err(FfiError::Message("$bigint must be a string".into())),
        };
    }
    if let Some(path) = map.remove(PATH_TAG) {
        return match path {
            Value::String(p) => Ok(MontyObject::Path(p)),
            _ => Err(FfiError::Message("$path must be a string".into())),
        };
    }
    if let Some(repr) = map.remove(REPR_TAG) {
        return match repr {
            Value::String(r) => Ok(MontyObject::Repr(r)),
            _ => Err(FfiError::Message("$repr must be a string".into())),
        };
    }
    if let Some(raw_exception) = map.remove(EXCEPTION_TAG) {
        return parse_exception(raw_exception);
    }
    if let Some(raw_dataclass) = map.remove(DATACLASS_TAG) {
        return parse_dataclass(raw_dataclass);
    }
    if let Some(raw_named_tuple) = map.remove(NAMED_TUPLE_TAG) {
        return parse_named_tuple(raw_named_tuple);
    }

    // Fallback: regular dict with string keys.
    let mut pairs = Vec::with_capacity(map.len());
    for (key, value) in map {
        let val = value_to_object(value)?;
        pairs.push((MontyObject::String(key), val));
    }
    Ok(MontyObject::Dict(DictPairs::from(pairs)))
}

fn parse_collection(value: Value) -> FfiResult<Vec<MontyObject>> {
    match value {
        Value::Array(items) => items.into_iter().map(value_to_object).collect(),
        _ => Err(FfiError::Message("expected array".into())),
    }
}

fn parse_dict(value: Value) -> FfiResult<DictPairs> {
    match value {
        Value::Array(items) => {
            let mut pairs = Vec::with_capacity(items.len());
            for entry in items {
                match entry {
                    Value::Array(mut parts) if parts.len() == 2 => {
                        let value = parts.pop().unwrap();
                        let key = parts.pop().unwrap();
                        let key_object = value_to_object(key)?;
                        let value_object = value_to_object(value)?;
                        pairs.push((key_object, value_object));
                    }
                    _ => return Err(FfiError::Message("invalid $dict entry".into())),
                }
            }
            Ok(DictPairs::from(pairs))
        }
        _ => Err(FfiError::Message("$dict must be an array".into())),
    }
}

fn parse_exception(value: Value) -> FfiResult<MontyObject> {
    let mut map = match value {
        Value::Object(m) => m,
        _ => return Err(FfiError::Message("$exception must be an object".into())),
    };
    let exc_type = map
        .remove("type")
        .and_then(|value| value.as_str().map(|s| s.to_owned()))
        .ok_or_else(|| FfiError::Message("$exception.type missing".into()))?;
    let message = map
        .remove("message")
        .and_then(|value| value.as_str().map(|s| s.to_owned()));
    let exc_type = exc_type
        .parse::<ExcType>()
        .map_err(|_| FfiError::Message("unknown exception type".into()))?;
    Ok(MontyObject::Exception {
        exc_type,
        arg: message,
    })
}

fn parse_dataclass(value: Value) -> FfiResult<MontyObject> {
    let map = match value {
        Value::Object(m) => m,
        _ => return Err(FfiError::Message("$dataclass must be an object".into())),
    };
    let name = map
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| FfiError::Message("$dataclass.name missing".into()))?
        .to_owned();
    let type_id = map
        .get("type_id")
        .and_then(Value::as_u64)
        .ok_or_else(|| FfiError::Message("$dataclass.type_id missing".into()))?;
    let field_names = map
        .get("field_names")
        .and_then(Value::as_array)
        .ok_or_else(|| FfiError::Message("$dataclass.field_names missing".into()))?
        .iter()
        .map(|v| v.as_str().map(|s| s.to_owned()))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| FfiError::Message("invalid field_names".into()))?;
    let attrs_value = map
        .get("attrs")
        .ok_or_else(|| FfiError::Message("$dataclass.attrs missing".into()))?
        .clone();
    let frozen = map.get("frozen").and_then(Value::as_bool).unwrap_or(false);
    let attrs = parse_dict(attrs_value)?;
    Ok(MontyObject::Dataclass {
        name,
        type_id,
        field_names,
        attrs,
        frozen,
    })
}

fn parse_named_tuple(value: Value) -> FfiResult<MontyObject> {
    let map = match value {
        Value::Object(m) => m,
        _ => return Err(FfiError::Message("$named_tuple must be an object".into())),
    };
    let type_name = map
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(|| FfiError::Message("$named_tuple.type missing".into()))?
        .to_owned();
    let field_names = map
        .get("field_names")
        .and_then(Value::as_array)
        .ok_or_else(|| FfiError::Message("$named_tuple.field_names missing".into()))?
        .iter()
        .map(|v| v.as_str().map(|s| s.to_owned()))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| FfiError::Message("invalid field_names".into()))?;
    let values = map
        .get("values")
        .and_then(Value::as_array)
        .ok_or_else(|| FfiError::Message("$named_tuple.values missing".into()))?
        .clone();
    let converted: FfiResult<Vec<_>> = values.into_iter().map(value_to_object).collect();
    Ok(MontyObject::NamedTuple {
        type_name,
        field_names,
        values: converted?,
    })
}

fn object_to_value(obj: &MontyObject) -> FfiResult<Value> {
    Ok(match obj {
        MontyObject::None => Value::Null,
        MontyObject::Bool(b) => Value::Bool(*b),
        MontyObject::Int(i) => Value::Number((*i).into()),
        MontyObject::Float(f) => json!(f),
        MontyObject::String(s) => Value::String(s.clone()),
        MontyObject::Bytes(bytes) => {
            let mut outer = Map::new();
            outer.insert(
                BYTES_TAG.into(),
                Value::Array(bytes.iter().map(|b| json!(b)).collect()),
            );
            Value::Object(outer)
        }
        MontyObject::List(items) => Value::Array(
            items
                .iter()
                .map(object_to_value)
                .collect::<FfiResult<Vec<_>>>()?,
        ),
        MontyObject::Tuple(items) => {
            let mut outer = Map::new();
            outer.insert(
                TUPLE_TAG.into(),
                Value::Array(
                    items
                        .iter()
                        .map(object_to_value)
                        .collect::<FfiResult<Vec<_>>>()?,
                ),
            );
            Value::Object(outer)
        }
        MontyObject::Dict(pairs) => {
            let mut outer = Map::new();
            outer.insert(
                DICT_TAG.into(),
                Value::Array(
                    pairs
                        .into_iter()
                        .map(|(k, v)| object_to_value_pair(k, v))
                        .collect::<FfiResult<Vec<_>>>()?,
                ),
            );
            Value::Object(outer)
        }
        MontyObject::Set(items) => encode_collection(SET_TAG, items)?,
        MontyObject::FrozenSet(items) => encode_collection(FROZEN_SET_TAG, items)?,
        MontyObject::Exception { exc_type, arg } => {
            let mut inner = Map::new();
            inner.insert("type".into(), Value::String(exc_type.to_string()));
            if let Some(message) = arg {
                inner.insert("message".into(), Value::String(message.clone()));
            }
            let mut outer = Map::new();
            outer.insert(EXCEPTION_TAG.into(), Value::Object(inner));
            Value::Object(outer)
        }
        MontyObject::Path(p) => {
            let mut outer = Map::new();
            outer.insert(PATH_TAG.into(), Value::String(p.clone()));
            Value::Object(outer)
        }
        MontyObject::Repr(r) => {
            let mut outer = Map::new();
            outer.insert(REPR_TAG.into(), Value::String(r.clone()));
            Value::Object(outer)
        }
        MontyObject::BigInt(value) => {
            let mut outer = Map::new();
            outer.insert(BIGINT_TAG.into(), Value::String(value.to_string()));
            Value::Object(outer)
        }
        MontyObject::Dataclass {
            name,
            type_id,
            field_names,
            attrs,
            frozen,
        } => {
            let mut inner = Map::new();
            inner.insert("name".into(), Value::String(name.clone()));
            inner.insert("type_id".into(), json!(type_id));
            inner.insert("field_names".into(), json!(field_names));
            inner.insert(
                "attrs".into(),
                Value::Array(
                    attrs
                        .into_iter()
                        .map(|(k, v)| object_to_value_pair(k, v))
                        .collect::<FfiResult<Vec<_>>>()?,
                ),
            );
            inner.insert("frozen".into(), Value::Bool(*frozen));
            let mut outer = Map::new();
            outer.insert(DATACLASS_TAG.into(), Value::Object(inner));
            Value::Object(outer)
        }
        MontyObject::NamedTuple {
            type_name,
            field_names,
            values,
        } => {
            let mut inner = Map::new();
            inner.insert("type".into(), Value::String(type_name.clone()));
            inner.insert("field_names".into(), json!(field_names));
            inner.insert(
                "values".into(),
                Value::Array(
                    values
                        .iter()
                        .map(object_to_value)
                        .collect::<FfiResult<Vec<_>>>()?,
                ),
            );
            let mut outer = Map::new();
            outer.insert(NAMED_TUPLE_TAG.into(), Value::Object(inner));
            Value::Object(outer)
        }
        MontyObject::Ellipsis => {
            let mut outer = Map::new();
            outer.insert(REPR_TAG.into(), Value::String("...".into()));
            Value::Object(outer)
        }
        MontyObject::Cycle(_, placeholder) => {
            let mut outer = Map::new();
            outer.insert(REPR_TAG.into(), Value::String(placeholder.clone()));
            Value::Object(outer)
        }
        _ => {
            let mut outer = Map::new();
            outer.insert(REPR_TAG.into(), Value::String(format!("{obj}")));
            Value::Object(outer)
        }
    })
}

fn encode_collection(tag: &str, items: &[MontyObject]) -> FfiResult<Value> {
    let mut outer = Map::new();
    outer.insert(
        tag.into(),
        Value::Array(
            items
                .iter()
                .map(object_to_value)
                .collect::<FfiResult<Vec<_>>>()?,
        ),
    );
    Ok(Value::Object(outer))
}

fn object_to_value_pair(key: &MontyObject, value: &MontyObject) -> FfiResult<Value> {
    Ok(Value::Array(vec![
        object_to_value(key)?,
        object_to_value(value)?,
    ]))
}

pub fn decode_value(value: Value) -> FfiResult<MontyObject> {
    value_to_object(value)
}
