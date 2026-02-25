#ifndef MONTY_FFI_H
#define MONTY_FFI_H

#include <stdarg.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdlib.h>

typedef struct MontyStatus {
  int32_t ok;
  char *error;
} MontyStatus;

typedef struct MontyRunHandle {
  void *inner;
} MontyRunHandle;

typedef struct SnapshotHandle {
  void *inner;
} SnapshotHandle;

typedef struct FutureSnapshotHandle {
  void *inner;
} FutureSnapshotHandle;

typedef struct ProgressResult {
  int32_t kind;
  char *result_json;
  char *function_name;
  char *os_function;
  char *args_json;
  char *kwargs_json;
  uint32_t call_id;
  int32_t method_call;
  struct SnapshotHandle *snapshot;
  char *pending_call_ids_json;
  struct FutureSnapshotHandle *future_snapshot;
} ProgressResult;

struct MontyStatus monty_run_new(const char *code,
                                 const char *script_name,
                                 const char *const *input_names,
                                 const char *const *ext_funcs,
                                 struct MontyRunHandle **out);

struct MontyStatus monty_run_dump(struct MontyRunHandle *run, uint8_t **out_bytes, size_t *out_len);

struct MontyStatus monty_run_load(const uint8_t *bytes, size_t len, struct MontyRunHandle **out);

void monty_run_free(struct MontyRunHandle *run);

struct MontyStatus monty_run_start(struct MontyRunHandle *run,
                                   const char *inputs_json,
                                   struct ProgressResult *out);

void monty_progress_result_free_strings(struct ProgressResult *result);

struct MontyStatus monty_snapshot_resume(struct SnapshotHandle *snapshot,
                                         uint32_t _call_id,
                                         const char *result_json,
                                         const char *error_message,
                                         struct ProgressResult *out);

struct MontyStatus monty_future_snapshot_resume(struct FutureSnapshotHandle *snapshot,
                                                const char *results_json,
                                                struct ProgressResult *out);

struct MontyStatus monty_snapshot_dump(struct SnapshotHandle *snapshot,
                                       uint8_t **out_bytes,
                                       size_t *out_len);

struct MontyStatus monty_snapshot_load(const uint8_t *bytes,
                                       size_t len,
                                       struct SnapshotHandle **out);

struct MontyStatus monty_future_snapshot_dump(struct FutureSnapshotHandle *snapshot,
                                              uint8_t **out_bytes,
                                              size_t *out_len);

struct MontyStatus monty_future_snapshot_load(const uint8_t *bytes,
                                              size_t len,
                                              struct FutureSnapshotHandle **out);

void monty_snapshot_free(struct SnapshotHandle *snapshot);

void monty_future_snapshot_free(struct FutureSnapshotHandle *snapshot);

void monty_free_bytes(uint8_t *ptr, size_t len);

void monty_free_string(char *s);

#endif  /* MONTY_FFI_H */
