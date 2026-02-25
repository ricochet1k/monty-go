# monty-go: Implementation Plan

## Architecture

A hybrid of opaque handles (for in-process fast-path) and binary blobs (for snapshot
persistence), with Rust doing all interpretation of internal state so Go only ever
sees clean JSON-encoded values and integer discriminants.

```
┌─────────────────────────────────────────────────────────┐
│  Go library  (pkg/monty)                                │
│  - Monty, Snapshot, FutureSnapshot types                │
│  - Run(), Start(), Resume(), Dump(), Load()             │
│  - MontyObject → Go types via JSON                      │
└────────────────────┬────────────────────────────────────┘
                     │ CGo
┌────────────────────▼────────────────────────────────────┐
│  monty-ffi  (Rust staticlib)                            │
│  - Opaque handles for MontyRun, Snapshot, FutureSnap    │
│  - Extracts RunProgress fields into JSON for Go         │
│  - dump()/load() expose handles as postcard []byte      │
└────────────────────┬────────────────────────────────────┘
                     │ Rust crate dep
┌────────────────────▼────────────────────────────────────┐
│  monty  (git dep: pydantic/monty)                       │
└─────────────────────────────────────────────────────────┘
```

## Repository Layout

```
monty-go/
├── monty-ffi/              # Rust staticlib wrapping monty
│   ├── Cargo.toml
│   ├── cbindgen.toml
│   └── src/
│       ├── lib.rs          # extern "C" functions
│       ├── error.rs        # error result type
│       └── json.rs         # MontyObject ↔ JSON helpers
├── pkg/
│   └── monty/              # Go library
│       ├── monty.go        # CGo imports + high-level API
│       ├── object.go       # MontyObject Go type + JSON decode
│       ├── resource.go     # ResourceLimits Go type
│       └── monty_test.go
├── include/
│   └── monty_ffi.h         # cbindgen-generated C header (committed)
├── Makefile                # builds staticlib, copies header, runs go test
└── PLAN.md
```

## Rust FFI Layer (`monty-ffi`)

### Handle types

Three opaque heap-allocated handle types. Go holds them as `unsafe.Pointer` wrapped in
typed Go structs with `runtime.SetFinalizer` for cleanup.

```
MontyRunHandle      ← Box<MontyRun>               (compiled code, cheaply cloned)
SnapshotHandle      ← Box<Snapshot<NoLimitTracker>>   (live execution state)
FutureSnapshotHandle← Box<FutureSnapshot<NoLimitTracker>>
```

> `NoLimitTracker` only for now. A follow-up can add `LimitedTracker` support via a
> `ResourceLimitsC` struct mirroring `ResourceLimits`.

### Error convention

Every function returns `MontyStatus`:

```c
typedef struct {
    int     ok;        // 1 = success, 0 = error
    char*   error;     // NULL on success; Rust-allocated, freed with monty_free_string()
} MontyStatus;
```

Out-parameters carry results. Caller checks `.ok` before using out-params.

### C API surface

#### Lifecycle

```c
// Compile Python source into a reusable handle.
// input_names: null-terminated array of C strings (variable names)
// ext_funcs:   null-terminated array of C strings (external function names)
MontyStatus monty_run_new(
    const char*  code,
    const char*  script_name,
    const char** input_names,
    const char** ext_funcs,
    MontyRunHandle** out
);

// Serialize a MontyRun to postcard bytes for caching.
MontyStatus monty_run_dump(MontyRunHandle* run, uint8_t** out_bytes, uintptr_t* out_len);

// Deserialize a MontyRun from postcard bytes.
MontyStatus monty_run_load(const uint8_t* bytes, uintptr_t len, MontyRunHandle** out);

void monty_run_free(MontyRunHandle* run);
```

#### Execution

```c
// Begin execution. inputs_json: JSON array of MontyObjects.
// Returns a ProgressResult (see below).
MontyStatus monty_run_start(
    MontyRunHandle*  run,       // borrowed, not consumed
    const char*      inputs_json,
    ProgressResult*  out
);
```

#### RunProgress result

Rather than a `ProgressHandle*`, execution returns a `ProgressResult` struct that
immediately tells Go what happened. If the result requires further interaction
(FunctionCall, OsCall, ResolveFutures), the relevant snapshot handle is included.
This avoids an extra round-trip just to query the kind.

```c
#define MONTY_PROGRESS_COMPLETE        0
#define MONTY_PROGRESS_FUNCTION_CALL   1
#define MONTY_PROGRESS_OS_CALL         2
#define MONTY_PROGRESS_RESOLVE_FUTURES 3

typedef struct {
    int kind;

    // MONTY_PROGRESS_COMPLETE: result_json is a JSON-encoded MontyObject
    char* result_json;

    // MONTY_PROGRESS_FUNCTION_CALL / OS_CALL:
    char*            function_name;   // NULL for OS calls (use os_function instead)
    char*            os_function;     // NULL for regular calls
    char*            args_json;       // JSON array of positional args
    char*            kwargs_json;     // JSON array of [key, value] pairs
    uint32_t         call_id;
    int              method_call;     // bool
    SnapshotHandle*  snapshot;        // ownership transferred to caller

    // MONTY_PROGRESS_RESOLVE_FUTURES:
    char*                  pending_call_ids_json;  // JSON array of uint32
    FutureSnapshotHandle*  future_snapshot;        // ownership transferred to caller
} ProgressResult;

// Free the string fields inside a ProgressResult (not the struct itself, which is stack-allocated).
// Does NOT free the snapshot/future_snapshot handles — caller owns those.
void monty_progress_result_free_strings(ProgressResult* r);
```

#### Resume

```c
// Resume from a FunctionCall snapshot with a return value or exception.
// result_json: JSON-encoded MontyObject, OR null + error_message to raise an exception.
// Consumes (and frees) the snapshot handle.
MontyStatus monty_snapshot_resume(
    SnapshotHandle*  snapshot,     // consumed
    uint32_t         call_id,
    const char*      result_json,
    const char*      error_message,  // NULL unless raising
    ProgressResult*  out
);

// Resume a FutureSnapshot with partial or full results.
// results_json: JSON array of {call_id: uint32, result: MontyObject|null, error: string|null}
// Consumes (and frees) the future snapshot handle.
MontyStatus monty_future_snapshot_resume(
    FutureSnapshotHandle* snapshot,   // consumed
    const char*           results_json,
    ProgressResult*       out
);
```

#### Snapshot serialization (for persistence)

```c
// Serialize a snapshot to postcard bytes. Does NOT consume the handle.
MontyStatus monty_snapshot_dump(SnapshotHandle* snap, uint8_t** out_bytes, uintptr_t* out_len);
MontyStatus monty_snapshot_load(const uint8_t* bytes, uintptr_t len, SnapshotHandle** out);

MontyStatus monty_future_snapshot_dump(FutureSnapshotHandle* snap, uint8_t** out_bytes, uintptr_t* out_len);
MontyStatus monty_future_snapshot_load(const uint8_t* bytes, uintptr_t len, FutureSnapshotHandle** out);

void monty_snapshot_free(SnapshotHandle* snap);
void monty_future_snapshot_free(FutureSnapshotHandle* snap);
```

#### Memory

```c
void monty_free_string(char* s);
void monty_free_bytes(uint8_t* ptr, uintptr_t len);
```

### JSON encoding of MontyObject

Monty already has serde support on `MontyObject`. The FFI layer calls
`serde_json::to_string()` for outputs and `serde_json::from_str::<MontyObject>()` for
inputs. The existing JSON mapping handles all primitive types cleanly. Extended types
(BigInt, Bytes, Ellipsis, Exception, Path, Dataclass) use the serde tagged format that
monty already defines — Go decodes these as `map[string]any` and the Go library provides
typed helpers.

---

## Go Library (`pkg/monty`)

### Core types

```go
// Monty holds compiled Python code. Safe to reuse across many calls.
// Finalizer calls monty_run_free on GC.
type Monty struct{ ptr unsafe.Pointer }

// Snapshot holds paused execution state after a FunctionCall or OsCall.
// Finalizer calls monty_snapshot_free if not yet consumed.
type Snapshot struct{ ptr unsafe.Pointer }

// FutureSnapshot holds paused execution state waiting on async futures.
type FutureSnapshot struct{ ptr unsafe.Pointer }
```

### API

```go
// New compiles Python code into a reusable Monty handle.
func New(code, scriptName string, inputNames, extFuncs []string) (*Monty, error)

// NewFromBytes loads a previously dumped Monty from postcard bytes.
func NewFromBytes(b []byte) (*Monty, error)

// Dump serializes the compiled code to postcard bytes for caching.
func (m *Monty) Dump() ([]byte, error)

// Run executes code to completion with no external functions.
// inputs are JSON-marshalable Go values.
func (m *Monty) Run(inputs ...any) (Object, error)

// Start begins iterative execution, returning a Progress.
func (m *Monty) Start(inputs ...any) (Progress, error)

// Progress is the result of Start or Resume.
type Progress struct {
    Kind         ProgressKind
    // Complete
    Result       Object
    // FunctionCall / OsCall
    FunctionName string
    OsFunction   string
    Args         []Object
    Kwargs       []KV
    CallID       uint32
    MethodCall   bool
    Snapshot     *Snapshot        // non-nil for FunctionCall/OsCall
    // ResolveFutures
    PendingIDs     []uint32
    FutureSnapshot *FutureSnapshot // non-nil for ResolveFutures
}

type ProgressKind int
const (
    Complete       ProgressKind = iota
    FunctionCall
    OsCall
    ResolveFutures
)

// Resume continues a paused snapshot with a return value or error.
func (s *Snapshot) Resume(callID uint32, result any) (Progress, error)
func (s *Snapshot) ResumeError(callID uint32, errMsg string) (Progress, error)

// Dump serializes snapshot state to bytes for persistence.
func (s *Snapshot) Dump() ([]byte, error)

// SnapshotFromBytes restores a snapshot from bytes.
func SnapshotFromBytes(b []byte) (*Snapshot, error)

// Resume resumes a future snapshot with partial or full results.
func (fs *FutureSnapshot) Resume(results []FutureResult) (Progress, error)

type FutureResult struct {
    CallID uint32
    Result any    // nil = still pending
    Err    string // non-empty = exception
}

func (fs *FutureSnapshot) PendingCallIDs() []uint32
func (fs *FutureSnapshot) Dump() ([]byte, error)
func FutureSnapshotFromBytes(b []byte) (*FutureSnapshot, error)
```

### Object type

`Object` is `any` in the API, backed by a discriminated Go type decoded from monty's
JSON output:

```go
type Object interface{ montyObject() }

type (
    None      struct{}
    Bool      bool
    Int       int64
    BigInt    *big.Int
    Float     float64
    String    string
    Bytes     []byte
    List      []Object
    Tuple     []Object
    Dict      []KV       // preserves order
    Set       []Object
    FrozenSet []Object
    Exception struct{ Type, Message string; Traceback []StackFrame }
    Repr      string     // output-only, non-roundtrippable values
    // ... Path, Dataclass, etc. as needed
)
type KV struct{ Key, Value Object }
```

---

## Build System

### Makefile targets

```makefile
.PHONY: build test clean

build: include/monty_ffi.h libmonty_ffi.a

include/monty_ffi.h: monty-ffi/src/lib.rs
    cd monty-ffi && cbindgen --config cbindgen.toml --output ../include/monty_ffi.h

libmonty_ffi.a: monty-ffi/src/lib.rs monty-ffi/Cargo.toml
    cd monty-ffi && cargo build --release
    cp monty-ffi/target/release/libmonty_ffi.a .

test: build
    go test ./pkg/monty/...

clean:
    rm -f libmonty_ffi.a include/monty_ffi.h
    cd monty-ffi && cargo clean
```

### CGo directives in `monty.go`

```go
// #cgo LDFLAGS: -L${SRCDIR}/../../ -lmonty_ffi -framework Security -framework Foundation
// #cgo CFLAGS: -I${SRCDIR}/../../include
// #include "monty_ffi.h"
import "C"
```

(Linux: drop `-framework` flags, add `-ldl -lpthread -lm`.)

The static library and header are committed to the repo so consumers don't need a Rust
toolchain — only the Makefile/CI workflow does.

---

## Phases

1. **monty-ffi crate** — implement all `extern "C"` functions, error handling, JSON
   helpers. Verify with a small C test or Rust integration test.
2. **Go Object type** — JSON decoder for `MontyObject` covering all variants.
3. **Go core API** — `New`, `Run`, `Start`, `Snapshot.Resume`, CGo glue.
4. **Go future/async API** — `FutureSnapshot`, `Resume` with partial results.
5. **Serialization** — `Dump`/`Load` for `Monty`, `Snapshot`, `FutureSnapshot`.
6. **ResourceLimits** — expose `LimitedTracker` via a `WithLimits(ResourceLimits)` option
   on `New` or `Start`.
7. **Tests + examples** — mirror the Python examples from monty's README.
