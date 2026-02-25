# monty-go

Go bindings for [pydantic/monty](https://github.com/pydantic/monty) that expose the
`MontyRun` async execution API via a lightweight C/Rust shim.

The repository contains two deliverables:

- `monty-ffi/`: Rust static library wrapping Monty’s iterators/snapshots.
- `pkg/monty`: idiomatic Go API that marshals values as JSON.

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

## Continuous Integration

GitHub Actions workflow (`.github/workflows/ci.yml`) builds the Rust static library and runs
`go test ./pkg/monty` for every push and pull request.

## Releasing

1. Run `make clean && make build && make test` locally.
2. Commit the regenerated `libmonty_ffi.a` and `include/monty_ffi.h`.
3. Tag a semantic version (e.g. `git tag v0.1.0 && git push origin v0.1.0`).
4. Create a GitHub Release for `ricochet1k/monty-go`; the Go module will pick up the version.
5. Attach each OS/arch-specific static library from `dist/<os>-<arch>/` plus `include/monty_ffi.h`
   so downstream projects can download the exact artifacts without building Rust themselves.

This mirrors Monty’s release cadence and gives Go consumers stable module versions while still
shipping the compiled Rust artifacts.
