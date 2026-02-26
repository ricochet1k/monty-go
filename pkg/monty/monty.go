package monty

/*
#cgo darwin,amd64 LDFLAGS: -L${SRCDIR}/../../dist/darwin-amd64 -lmonty_ffi -framework Security -framework Foundation
#cgo darwin,arm64 LDFLAGS: -L${SRCDIR}/../../dist/darwin-arm64 -lmonty_ffi -framework Security -framework Foundation
#cgo darwin CFLAGS: -I${SRCDIR}/../../include
#cgo linux,amd64 LDFLAGS: -L${SRCDIR}/../../dist/linux-amd64 -lmonty_ffi -ldl -lpthread -lm
#cgo linux,arm64 LDFLAGS: -L${SRCDIR}/../../dist/linux-arm64 -lmonty_ffi -ldl -lpthread -lm
#cgo linux CFLAGS: -I${SRCDIR}/../../include
#include <stdlib.h>
#include "monty_ffi.h"
*/
import "C"

import (
	"encoding/json"
	"errors"
	"fmt"
	"runtime"
	"unsafe"
)

// ProgressKind mirrors the C enum constants.
type ProgressKind int

const (
	Complete ProgressKind = iota
	FunctionCall
	OsCall
	ResolveFutures
)

// Progress represents the result of a start/resume call.
type Progress struct {
	Kind           ProgressKind
	Result         Object
	FunctionName   string
	OsFunction     string
	Args           []Object
	Kwargs         []KV
	CallID         uint32
	MethodCall     bool
	Snapshot       *Snapshot
	PendingIDs     []uint32
	FutureSnapshot *FutureSnapshot
}

// FutureResult matches the JSON shape accepted by monty_future_snapshot_resume.
type FutureResult struct {
	CallID uint32
	Result any
	Err    string
}

// Monty wraps a compiled MontyRun handle.
type Monty struct {
	handle *C.MontyRunHandle
}

// Snapshot holds a paused synchronous execution state.
type Snapshot struct {
	handle *C.SnapshotHandle
}

// FutureSnapshot holds a paused async execution state.
type FutureSnapshot struct {
	handle  *C.FutureSnapshotHandle
	pending []uint32
}

// New compiles Python code into a Monty handle.
func New(code, scriptName string, inputNames, extFuncs []string) (*Monty, error) {
	cCode, freeCode := cString(code)
	defer freeCode()
	cScript, freeScript := cString(scriptName)
	defer freeScript()
	inputs, freeInputs := cStringArray(inputNames)
	defer freeInputs()
	exts, freeExts := cStringArray(extFuncs)
	defer freeExts()

	var out *C.MontyRunHandle
	status := C.monty_run_new(cCode, cScript, (**C.char)(inputs), (**C.char)(exts), &out)
	if err := statusError(status); err != nil {
		return nil, err
	}
	return newMonty(out), nil
}

// NewFromBytes restores a Monty handle from postcard bytes.
func NewFromBytes(data []byte) (*Monty, error) {
	if len(data) == 0 {
		return nil, errors.New("monty: empty snapshot")
	}
	var out *C.MontyRunHandle
	status := C.monty_run_load((*C.uint8_t)(unsafe.Pointer(&data[0])), C.size_t(len(data)), &out)
	if err := statusError(status); err != nil {
		return nil, err
	}
	return newMonty(out), nil
}

// Dump serializes the compiled Monty run to postcard bytes.
func (m *Monty) Dump() ([]byte, error) {
	if m == nil || m.handle == nil {
		return nil, errors.New("monty: nil handle")
	}
	var buf *C.uint8_t
	var length C.size_t
	status := C.monty_run_dump(m.handle, &buf, &length)
	if err := statusError(status); err != nil {
		return nil, err
	}
	return copyBytes(buf, length), nil
}

// Run executes code to completion in one shot.
func (m *Monty) Run(inputs ...any) (Object, error) {
	progress, err := m.Start(inputs...)
	if err != nil {
		return nil, err
	}
	if progress.Kind != Complete {
		return nil, fmt.Errorf("monty: execution paused unexpectedly (%v)", progress.Kind)
	}
	return progress.Result, nil
}

// Start begins execution and returns the first progress result.
func (m *Monty) Start(inputs ...any) (Progress, error) {
	if m == nil || m.handle == nil {
		return Progress{}, errors.New("monty: nil handle")
	}
	payload, freePayload, err := marshalInputs(inputs)
	if err != nil {
		return Progress{}, err
	}
	defer freePayload()

	var raw C.ProgressResult
	status := C.monty_run_start(m.handle, payload, &raw)
	defer C.monty_progress_result_free_strings(&raw)
	if err := statusError(status); err != nil {
		return Progress{}, err
	}
	return convertProgress(&raw)
}

// Close releases the underlying Monty handle.
func (m *Monty) Close() {
	if m != nil && m.handle != nil {
		C.monty_run_free(m.handle)
		m.handle = nil
	}
}

// SnapshotFromBytes restores a snapshot from postcard bytes.
func SnapshotFromBytes(data []byte) (*Snapshot, error) {
	if len(data) == 0 {
		return nil, errors.New("monty: empty snapshot bytes")
	}
	var out *C.SnapshotHandle
	status := C.monty_snapshot_load((*C.uint8_t)(unsafe.Pointer(&data[0])), C.size_t(len(data)), &out)
	if err := statusError(status); err != nil {
		return nil, err
	}
	return newSnapshot(out), nil
}

// FutureSnapshotFromBytes restores a future snapshot from postcard bytes.
func FutureSnapshotFromBytes(data []byte) (*FutureSnapshot, error) {
	if len(data) == 0 {
		return nil, errors.New("monty: empty snapshot bytes")
	}
	var out *C.FutureSnapshotHandle
	status := C.monty_future_snapshot_load((*C.uint8_t)(unsafe.Pointer(&data[0])), C.size_t(len(data)), &out)
	if err := statusError(status); err != nil {
		return nil, err
	}
	return newFutureSnapshot(out, nil), nil
}

// Dump serializes the snapshot without consuming it.
func (s *Snapshot) Dump() ([]byte, error) {
	if s == nil || s.handle == nil {
		return nil, errors.New("monty: snapshot closed")
	}
	var buf *C.uint8_t
	var length C.size_t
	status := C.monty_snapshot_dump(s.handle, &buf, &length)
	if err := statusError(status); err != nil {
		return nil, err
	}
	return copyBytes(buf, length), nil
}

// Dump serializes the future snapshot without consuming it.
func (fs *FutureSnapshot) Dump() ([]byte, error) {
	if fs == nil || fs.handle == nil {
		return nil, errors.New("monty: future snapshot closed")
	}
	var buf *C.uint8_t
	var length C.size_t
	status := C.monty_future_snapshot_dump(fs.handle, &buf, &length)
	if err := statusError(status); err != nil {
		return nil, err
	}
	return copyBytes(buf, length), nil
}

// PendingCallIDs returns the cached pending call IDs for the snapshot.
func (fs *FutureSnapshot) PendingCallIDs() []uint32 {
	if fs == nil {
		return nil
	}
	return append([]uint32(nil), fs.pending...)
}

// Resume continues execution of a function call with a result value.
func (s *Snapshot) Resume(callID uint32, result any) (Progress, error) {
	return s.resume(callID, result, "")
}

// ResumeError continues execution by raising an exception message.
func (s *Snapshot) ResumeError(callID uint32, message string) (Progress, error) {
	if message == "" {
		return Progress{}, errors.New("monty: empty error message")
	}
	return s.resume(callID, nil, message)
}

// ResumeFuture continues execution treating the call as pending (returns ExternalFuture).
func (s *Snapshot) ResumeFuture(callID uint32) (Progress, error) {
	return s.resume(callID, nil, "")
}

func (s *Snapshot) resume(callID uint32, result any, errMsg string) (Progress, error) {
	if s == nil || s.handle == nil {
		return Progress{}, errors.New("monty: snapshot closed")
	}
	var resultJSON *C.char
	var freeResult func()
	var err error
	if errMsg == "" && result != nil {
		resultJSON, freeResult, err = marshalValue(result)
		if err != nil {
			return Progress{}, err
		}
		defer freeResult()
	}

	var errC *C.char
	var freeErr func()
	if errMsg != "" {
		errC, freeErr = cString(errMsg)
		defer freeErr()
	}

	var raw C.ProgressResult
	status := C.monty_snapshot_resume(s.handle, C.uint32_t(callID), resultJSON, errC, &raw)
	s.handle = nil
	defer C.monty_progress_result_free_strings(&raw)
	if err := statusError(status); err != nil {
		return Progress{}, err
	}
	return convertProgress(&raw)
}

// Resume resumes futures with provided results.
func (fs *FutureSnapshot) Resume(results []FutureResult) (Progress, error) {
	if fs == nil || fs.handle == nil {
		return Progress{}, errors.New("monty: future snapshot closed")
	}
	payload, freePayload, err := marshalFutureResults(results)
	if err != nil {
		return Progress{}, err
	}
	defer freePayload()

	var raw C.ProgressResult
	status := C.monty_future_snapshot_resume(fs.handle, payload, &raw)
	fs.handle = nil
	defer C.monty_progress_result_free_strings(&raw)
	if err := statusError(status); err != nil {
		return Progress{}, err
	}
	return convertProgress(&raw)
}

// Close frees the snapshot handle.
func (s *Snapshot) Close() {
	if s != nil && s.handle != nil {
		C.monty_snapshot_free(s.handle)
		s.handle = nil
	}
}

// Close frees the future snapshot handle.
func (fs *FutureSnapshot) Close() {
	if fs != nil && fs.handle != nil {
		C.monty_future_snapshot_free(fs.handle)
		fs.handle = nil
		fs.pending = nil
	}
}

func newMonty(handle *C.MontyRunHandle) *Monty {
	m := &Monty{handle: handle}
	runtime.SetFinalizer(m, func(m *Monty) { m.Close() })
	return m
}

func newSnapshot(handle *C.SnapshotHandle) *Snapshot {
	snap := &Snapshot{handle: handle}
	runtime.SetFinalizer(snap, func(s *Snapshot) { s.Close() })
	return snap
}

func newFutureSnapshot(handle *C.FutureSnapshotHandle, pending []uint32) *FutureSnapshot {
	fs := &FutureSnapshot{handle: handle, pending: pending}
	runtime.SetFinalizer(fs, func(fs *FutureSnapshot) { fs.Close() })
	return fs
}

func copyBytes(buf *C.uint8_t, length C.size_t) []byte {
	if buf == nil || length == 0 {
		return nil
	}
	goBuf := C.GoBytes(unsafe.Pointer(buf), C.int(length))
	C.monty_free_bytes(buf, length)
	return goBuf
}

func marshalInputs(values []any) (*C.char, func(), error) {
	data, err := json.Marshal(values)
	if err != nil {
		return nil, nil, err
	}
	str, free := cBytes(data)
	return str, free, nil
}

func marshalValue(value any) (*C.char, func(), error) {
	normalized, err := normalizeValue(value)
	if err != nil {
		return nil, nil, err
	}
	data, err := json.Marshal(normalized)
	if err != nil {
		return nil, nil, err
	}
	str, free := cBytes(data)
	return str, free, nil
}

func marshalFutureResults(results []FutureResult) (*C.char, func(), error) {
	payload := make([]map[string]any, 0, len(results))
	for _, item := range results {
		entry := map[string]any{"call_id": item.CallID}
		if item.Err != "" {
			entry["error"] = item.Err
		} else if item.Result != nil {
			normalized, err := normalizeValue(item.Result)
			if err != nil {
				return nil, nil, err
			}
			entry["result"] = normalized
		}
		payload = append(payload, entry)
	}
	data, err := json.Marshal(payload)
	if err != nil {
		return nil, nil, err
	}
	str, free := cBytes(data)
	return str, free, nil
}

func normalizeValue(value any) (any, error) {
	if obj, ok := value.(Object); ok {
		return objectToInterface(obj)
	}
	switch v := value.(type) {
	case []Object:
		elems := make([]any, len(v))
		for i, item := range v {
			normalized, err := objectToInterface(item)
			if err != nil {
				return nil, err
			}
			elems[i] = normalized
		}
		return elems, nil
	default:
		return value, nil
	}
}

func convertProgress(raw *C.ProgressResult) (Progress, error) {
	progress := Progress{
		Kind:       ProgressKind(raw.kind),
		CallID:     uint32(raw.call_id),
		MethodCall: raw.method_call != 0,
	}

	if raw.result_json != nil {
		obj, err := decodeObjectString(C.GoString(raw.result_json))
		if err != nil {
			return Progress{}, err
		}
		progress.Result = obj
	}
	if raw.function_name != nil {
		progress.FunctionName = C.GoString(raw.function_name)
	}
	if raw.os_function != nil {
		progress.OsFunction = C.GoString(raw.os_function)
	}
	if raw.args_json != nil {
		args, err := decodeObjectArrayString(C.GoString(raw.args_json))
		if err != nil {
			return Progress{}, err
		}
		progress.Args = args
	}
	if raw.kwargs_json != nil {
		kwargs, err := decodeKwargsString(C.GoString(raw.kwargs_json))
		if err != nil {
			return Progress{}, err
		}
		progress.Kwargs = kwargs
	}
	if raw.pending_call_ids_json != nil {
		ids, err := decodeUint32ArrayString(C.GoString(raw.pending_call_ids_json))
		if err != nil {
			return Progress{}, err
		}
		progress.PendingIDs = ids
	}
	if raw.snapshot != nil {
		progress.Snapshot = newSnapshot(raw.snapshot)
		raw.snapshot = nil
	}
	if raw.future_snapshot != nil {
		progress.FutureSnapshot = newFutureSnapshot(raw.future_snapshot, progress.PendingIDs)
		raw.future_snapshot = nil
	}
	return progress, nil
}

func cString(value string) (*C.char, func()) {
	cstr := C.CString(value)
	return cstr, func() {
		C.free(unsafe.Pointer(cstr))
	}
}

func cBytes(data []byte) (*C.char, func()) {
	if len(data) == 0 {
		cstr := C.CString("")
		return cstr, func() { C.free(unsafe.Pointer(cstr)) }
	}
	cstr := C.CString(string(data))
	return cstr, func() { C.free(unsafe.Pointer(cstr)) }
}

func cStringArray(values []string) (**C.char, func()) {
	if len(values) == 0 {
		return nil, func() {}
	}
	items := make([]*C.char, len(values)+1)
	for i, v := range values {
		items[i] = C.CString(v)
	}
	items[len(values)] = nil
	return (**C.char)(unsafe.Pointer(&items[0])), func() {
		for _, ptr := range items[:len(values)] {
			C.free(unsafe.Pointer(ptr))
		}
	}
}

func statusError(status C.MontyStatus) error {
	if status.ok != 0 {
		return nil
	}
	var message string
	if status.error != nil {
		message = C.GoString(status.error)
		C.monty_free_string(status.error)
	} else {
		message = "monty: unknown error"
	}
	return errors.New(message)
}
