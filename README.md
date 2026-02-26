# monty-go

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

func main() {
    runner, err := monty.New("x + 1", "adder.py", []string{"x"}, nil)
    if err != nil {
        panic(err)
    }
    defer runner.Close()

    progress, err := runner.Start(41)
    if err != nil {
        panic(err)
    }
    if progress.Kind != monty.Complete {
        panic("unexpected pause")
    }

    var value int
    if err := progress.Result.Unmarshal(&value); err != nil {
        panic(err)
    }
    fmt.Println("result:", value) // prints 42
}
```

## API Overview

| Type | Purpose |
| --- | --- |
| `monty.Monty` | Compiled Monty run. `New`, `NewFromBytes`, `Run`, `Start`, `Dump`, `Close`. |
| `monty.Progress` | Result of `Start` or a Resume call. Includes `Kind`, decoded args, and snapshot handles. |
| `monty.Snapshot` | Paused call/OS snapshot. Supports `Resume`, `ResumeError`, `Dump`, `Close`. |
| `monty.FutureSnapshot` | Paused async/future state. `Resume`, `PendingCallIDs`, `Dump`, `Close`. |
| `monty.Object` | JSON-backed representation of a Monty value. Use `Unmarshal()` to decode. |

Snapshots/futures use `runtime.SetFinalizer` to free handles, but it’s still best practice to
call `Close()` when you’re done.

## Releasing

1. Run `make clean && make build && make test` locally.
2. Commit the regenerated `libmonty_ffi.a` and `include/monty_ffi.h`.
3. Tag a semantic version (e.g. `git tag v0.1.0 && git push origin v0.1.0`).
4. Create a GitHub Release for `ricochet1k/monty-go`; the Go module will pick up the version.
5. Attach each OS/arch-specific static library from `dist/<os>-<arch>/` plus `include/monty_ffi.h`
   so downstream projects can download the exact artifacts without building Rust themselves.

This mirrors Monty’s release cadence and gives Go consumers stable module versions while still
shipping the compiled Rust artifacts.
