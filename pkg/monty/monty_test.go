package monty

import "testing"

func TestMontyRunComplete(t *testing.T) {
	m := newTestMonty(t, "x + 1", []string{"x"}, nil)

	progress, err := m.Start(41)
	if err != nil {
		t.Fatalf("Start failed: %v", err)
	}
	if progress.Kind != Complete {
		t.Fatalf("expected Complete progress, got %v", progress.Kind)
	}
	var got int
	if err := progress.Result.Unmarshal(&got); err != nil {
		t.Fatalf("unmarshal result: %v", err)
	}
	if got != 42 {
		t.Fatalf("expected 42, got %d", got)
	}
}

func TestSnapshotResume(t *testing.T) {
	m := newTestMonty(t, "add_one(x)", []string{"x"}, []string{"add_one"})

	progress, err := m.Start(5)
	if err != nil {
		t.Fatalf("Start failed: %v", err)
	}
	if progress.Kind != FunctionCall {
		t.Fatalf("expected FunctionCall progress, got %v", progress.Kind)
	}
	if progress.Snapshot == nil {
		t.Fatalf("expected snapshot for function call")
	}
	if len(progress.Args) != 1 {
		t.Fatalf("expected single arg, got %d", len(progress.Args))
	}
	var arg int
	if err := progress.Args[0].Unmarshal(&arg); err != nil {
		t.Fatalf("unmarshal arg: %v", err)
	}
	if arg != 5 {
		t.Fatalf("expected arg 5, got %d", arg)
	}

	next, err := progress.Snapshot.Resume(progress.CallID, 11)
	if err != nil {
		t.Fatalf("Resume failed: %v", err)
	}
	if next.Kind != Complete {
		t.Fatalf("expected Complete progress, got %v", next.Kind)
	}
	var got int
	if err := next.Result.Unmarshal(&got); err != nil {
		t.Fatalf("unmarshal resumed result: %v", err)
	}
	if got != 11 {
		t.Fatalf("expected resumed result 11, got %d", got)
	}
}

func newTestMonty(t *testing.T, code string, inputs, exts []string) *Monty {
	t.Helper()
	m, err := New(code, "test.py", inputs, exts)
	if err != nil {
		t.Fatalf("New failed: %v", err)
	}
	t.Cleanup(func() { m.Close() })
	return m
}
