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
- [ ] CLI tooling for compiling Monty bytecode ahead of time.

## Prerequisites

You need:

- Go 1.21+
- Rust toolchain + `cargo`
- [`cbindgen`](https://github.com/eqrion/cbindgen) to regenerate the header

## Installation

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

Every GitHub Release includes tarballs named `monty-go-<version>-<os>-<arch>.tar.gz`.
To consume an official build without installing Rust:

```bash
VERSION=v0.1.0
PLATFORM=darwin-arm64   # choose the archive that matches GOOS-GOARCH

curl -L -O https://github.com/ricochet1k/monty-go/releases/download/${VERSION}/monty-go-${VERSION}-${PLATFORM}.tar.gz
tar -xzf monty-go-${VERSION}-${PLATFORM}.tar.gz
mkdir -p dist/${PLATFORM}
cp libmonty_ffi.a dist/${PLATFORM}/
mkdir -p include
cp monty_ffi.h include/

go get github.com/ricochet1k/monty-go/pkg/monty@${VERSION}
```

Now the `pkg/monty` `#cgo` directives will find the static library under `dist/<os>-<arch>`
and the header under `include/` when you build your Go program.

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
