# monty-go

[![Go Reference](https://pkg.go.dev/badge/github.com/ricochet1k/monty-go/pkg/monty.svg)](https://pkg.go.dev/github.com/ricochet1k/monty-go/pkg/monty)
[![Go Report Card](https://goreportcard.com/badge/github.com/ricochet1k/monty-go)](https://goreportcard.com/report/github.com/ricochet1k/monty-go)
[![CI](https://github.com/ricochet1k/monty-go/actions/workflows/ci.yml/badge.svg)](https://github.com/ricochet1k/monty-go/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/ricochet1k/monty-go.svg?logo=github)](https://github.com/ricochet1k/monty-go/releases)

Go bindings for [pydantic/monty](https://github.com/pydantic/monty), the experimental
Python interpreter focused on reproducible async execution. Monty compiles Python code
once, lets you intercept every external/OS call, snapshot execution state, and resume it
later. `monty-go` packages Monty as a tiny Rust static library and layers an idiomatic Go
API on top so Go programs can run embedded Python safely.

The repository contains two deliverables:

- `monty-ffi/`: Rust static library wrapping Monty’s iterators/snapshots.
- `pkg/monty`: idiomatic Go API that marshals values as JSON.

## Features

- [x] Compile Python source into reusable `Monty` handles.
- [x] Intercept external function calls/OS operations with resumable snapshots.
- [x] Serialize `Monty`, `Snapshot`, and `FutureSnapshot` handles to postcard bytes.
- [x] JSON-backed `Object` values with helpers for positional/keyword args.
- [x] Prebuilt static libraries for darwin/linux on amd64/arm64.
- [ ] Resource limiter configuration (Monty’s `LimitedTracker`).
- [ ] Strongly typed Go wrappers for common MontyObject variants.
- [ ] Run more code in the same environment after it finishes (blocked on https://github.com/pydantic/monty/issues/190)

## Prerequisites

You need:

- Go 1.21+
- Rust toolchain + `cargo`
- [`cbindgen`](https://github.com/eqrion/cbindgen) to regenerate the header

## Installation (build from source)

```bash
# clone the repository
git clone https://github.com/ricochet1k/monty-go.git
cd monty-go

# build the Rust static library and header (outputs to dist/<platform>)
make build

# run the Go tests
make test
```

The `make build` target produces `dist/<os>-<arch>/libmonty_ffi.a` (e.g. `dist/darwin-amd64/`
or `dist/linux-arm64/`) alongside `include/monty_ffi.h`. Both paths are consumed by the Go
module; having separate directories keeps every OS/architecture artifact side-by-side.

## Using a released build

Each GitHub Release ships `monty-go-<version>-<os>-<arch>.tar.gz`. Those archives contain
the `dist/<os>-<arch>/libmonty_ffi.a` and `include/monty_ffi.h` tree that `pkg/monty`
expects beside its Go sources. A practical workflow is:

```bash
VERSION=v0.1.0
PLATFORM=darwin-arm64   # pick the archive matching GOOS-GOARCH

# Vendor the Go sources so you control the dist/include directories
mkdir -p third_party
git clone https://github.com/ricochet1k/monty-go third_party/monty-go
git -C third_party/monty-go checkout ${VERSION}

# Overlay the release artifacts (populates dist/<platform>/ and include/)
curl -L -O https://github.com/ricochet1k/monty-go/releases/download/${VERSION}/monty-go-${VERSION}-${PLATFORM}.tar.gz
tar -xzf monty-go-${VERSION}-${PLATFORM}.tar.gz -C third_party/monty-go

# Make your module prefer the vendored copy with matching artifacts
go mod edit -replace github.com/ricochet1k/monty-go=./third_party/monty-go
go get github.com/ricochet1k/monty-go/pkg/monty@${VERSION}
```

You can add `third_party/monty-go` as a git submodule or vendor directory in your project.
Building your application (`go build`, `go run`, etc.) will now link against the vendored
static library automatically via the `#cgo` directives.

## Usage

```go
package main

import (
    "fmt"

    "github.com/ricochet1k/monty-go/pkg/monty"
)

const script = `external_add(x, 10) * 2`

func main() {
    runner, err := monty.New(script, "sample.py", []string{}, []string{"external_add"})
    if err != nil {
        panic(err)
    }
    defer runner.Close()

    progress, err := runner.Start(11)
    if err != nil {
        panic(err)
    }

    if progress.Kind != monty.FunctionCall {
        panic("expected an external function call")
    }
    fmt.Println("Monty requested:", progress.FunctionName, "args:", len(progress.Args))

    // Emulate the host performing the external work and resume the VM
    resumed, err := progress.Snapshot.Resume(progress.CallID, 32)
    if err != nil {
        panic(err)
    }

    if resumed.Kind != monty.Complete {
        panic("expected completion")
    }
    var result int
    if err := resumed.Result.Unmarshal(&result); err != nil {
        panic(err)
    }
    fmt.Println("result:", result)
}
```

## API Overview

### Monty handles and inputs

```go
m, _ := monty.New(code, "script.py", []string{"x", "y"}, []string{"external_add"})
progress, _ := m.Start(11, 5) // x=11, y=5
```

`Monty` instances are compiled bytecode. Pass `inputNames` when calling `New`, then provide
matching values to `Start`/`Run`. When execution pauses, a `Progress` describes the state.

### Progress kinds

```go
switch progress.Kind {
case monty.Complete:
    var out int
    _ = progress.Result.Unmarshal(&out)
case monty.FunctionCall:
    fmt.Println("external", progress.FunctionName, "args", len(progress.Args))
    next, _ := progress.Snapshot.Resume(progress.CallID, hostResult)
case monty.OsCall:
    // call into your own sandboxed OS layer, then Resume/ResumeError
case monty.ResolveFutures:
    pending := progress.FutureSnapshot.PendingCallIDs()
    next, _ := progress.FutureSnapshot.Resume([]monty.FutureResult{{CallID: pending[0], Result: 42}})
}
```

`Progress.Result`, `.Args`, `.Kwargs`, etc., use the `Object` wrapper—decode them with
`Object.Unmarshal(&target)`.

### Snapshots vs. runners

`Snapshot.Resume` lives on the snapshot because it holds the suspended VM state. You only
call `Monty.Start` once per run; every subsequent resume goes through the snapshot handle.

```go
next, err := progress.Snapshot.Resume(progress.CallID, 123)
nextErr, err := progress.Snapshot.ResumeError(progress.CallID, "boom")
raw := progress.Snapshot.Dump()           // []byte, postcard encoded
snapAgain, _ := monty.SnapshotFromBytes(raw)
```

### Futures

If you return `monty.FutureSnapshot`, resume it with a list describing which async call IDs
are ready:

```go
pending := progress.PendingIDs
updates := []monty.FutureResult{{CallID: pending[0], Result: map[string]any{"ok": true}}}
next, err := progress.FutureSnapshot.Resume(updates)
```

Each `FutureResult` can set `Result`, `Err`, or leave both empty to keep waiting.

### Objects in/out

Inputs you pass to `New`/`Start` just need to be JSON-serializable. To send a custom object
back into Monty, you can pre-marshal it:

```go
payload := map[string]any{"counts": []int{1, 2, 3}}
next, _ := progress.Snapshot.Resume(progress.CallID, payload)
```

For outputs, call `Object.Unmarshal(&target)` (or use `encoding/json` manually) to decode.

### Dump/load

`Monty`, `Snapshot`, and `FutureSnapshot` can be serialized to postcard bytes for caching
between processes:

```go
blob, _ := m.Dump()
mAgain, _ := monty.NewFromBytes(blob)

snapBytes, _ := progress.Snapshot.Dump()
snapRestored, _ := monty.SnapshotFromBytes(snapBytes)
```

Snapshots/futures use `runtime.SetFinalizer`, but it’s still best practice to call `Close()`
when you’re done with a handle.

## Releasing

1. Run `make clean && make build && make test` locally.
2. Commit the regenerated `libmonty_ffi.a` and `include/monty_ffi.h`.
3. Tag a semantic version (e.g. `git tag v0.1.0 && git push origin v0.1.0`).
4. Create a GitHub Release for `ricochet1k/monty-go`; the Go module will pick up the version.
5. Attach each OS/arch-specific static library from `dist/<os>-<arch>/` plus `include/monty_ffi.h`
   so downstream projects can download the exact artifacts without building Rust themselves.

This mirrors Monty’s release cadence and gives Go consumers stable module versions while still
shipping the compiled Rust artifacts.
