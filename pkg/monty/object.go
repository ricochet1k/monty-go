package monty

import (
    "encoding/json"
    "fmt"
)

// Object is a thin wrapper around JSON returned by the FFI layer.
type Object []byte

func (Object) montyObject() {}

// KV represents a keyword argument pair.
type KV struct {
    Key   Object
    Value Object
}

// Unmarshal decodes the JSON payload into the provided target.
func (o Object) Unmarshal(target any) error {
    if len(o) == 0 {
        return fmt.Errorf("monty: empty object payload")
    }
    return json.Unmarshal(o, target)
}

func decodeObjectString(s string) (Object, error) {
    if s == "" {
        return nil, nil
    }
    return Object([]byte(s)), nil
}

func decodeObjectArrayString(s string) ([]Object, error) {
    if s == "" {
        return nil, nil
    }
    var raw []json.RawMessage
    if err := json.Unmarshal([]byte(s), &raw); err != nil {
        return nil, err
    }
    out := make([]Object, len(raw))
    for i, item := range raw {
        out[i] = append(Object{}, item...)
    }
    return out, nil
}

func decodeKwargsString(s string) ([]KV, error) {
    if s == "" {
        return nil, nil
    }
    var raw [][]json.RawMessage
    if err := json.Unmarshal([]byte(s), &raw); err != nil {
        return nil, err
    }
    kvs := make([]KV, len(raw))
    for i, pair := range raw {
        if len(pair) != 2 {
            return nil, fmt.Errorf("monty: invalid kwargs entry")
        }
        kvs[i] = KV{
            Key:   append(Object{}, pair[0]...),
            Value: append(Object{}, pair[1]...),
        }
    }
    return kvs, nil
}

func decodeUint32ArrayString(s string) ([]uint32, error) {
    if s == "" {
        return nil, nil
    }
    var ids []uint32
    if err := json.Unmarshal([]byte(s), &ids); err != nil {
        return nil, err
    }
    return ids, nil
}

func objectToInterface(obj Object) (any, error) {
    if len(obj) == 0 {
        return nil, nil
    }
    var value any
    if err := json.Unmarshal(obj, &value); err != nil {
        return nil, err
    }
    return value, nil
}
