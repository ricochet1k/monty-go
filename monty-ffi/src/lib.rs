mod error;
mod json;

use std::{ffi::c_void, os::raw::c_char, ptr, slice};

use error::{
    monty_free_string, read_optional_str, read_required_str, to_c_string, FfiError, FfiResult,
    MontyStatus,
};
use json::{
    decode_inputs, decode_object, decode_value, encode_kwargs, encode_object, encode_objects,
    encode_u32_slice,
};
use monty::{
    ExcType, ExternalResult, FutureSnapshot, MontyException, MontyRun, NoLimitTracker, PrintWriter,
    RunProgress, Snapshot,
};
use postcard::{from_bytes, to_allocvec};
use serde::Deserialize;
use serde_json::Value;

#[repr(C)]
pub struct MontyRunHandle {
    inner: *mut c_void,
}

impl MontyRunHandle {
    fn as_ref(&self) -> &MontyRun {
        unsafe { &*(self.inner as *mut MontyRun) }
    }

    fn new(runner: MontyRun) -> *mut Self {
        let boxed = Box::new(runner);
        Box::into_raw(Box::new(Self {
            inner: Box::into_raw(boxed) as *mut c_void,
        }))
    }
}

#[repr(C)]
pub struct SnapshotHandle {
    inner: *mut c_void,
}

impl SnapshotHandle {
    fn as_ref(&self) -> &Snapshot<NoLimitTracker> {
        unsafe { &*(self.inner as *mut Snapshot<NoLimitTracker>) }
    }

    fn into_inner(self: Box<Self>) -> Snapshot<NoLimitTracker> {
        unsafe { *Box::from_raw(self.inner as *mut Snapshot<NoLimitTracker>) }
    }

    fn new(snapshot: Snapshot<NoLimitTracker>) -> *mut Self {
        let boxed = Box::new(snapshot);
        Box::into_raw(Box::new(Self {
            inner: Box::into_raw(boxed) as *mut c_void,
        }))
    }
}

#[repr(C)]
pub struct FutureSnapshotHandle {
    inner: *mut c_void,
}

impl FutureSnapshotHandle {
    fn pending_ids(&self) -> &[u32] {
        self.as_ref().pending_call_ids()
    }

    fn into_inner(self: Box<Self>) -> FutureSnapshot<NoLimitTracker> {
        unsafe { *Box::from_raw(self.inner as *mut FutureSnapshot<NoLimitTracker>) }
    }

    fn new(snapshot: FutureSnapshot<NoLimitTracker>) -> *mut Self {
        let boxed = Box::new(snapshot);
        Box::into_raw(Box::new(Self {
            inner: Box::into_raw(boxed) as *mut c_void,
        }))
    }

    fn as_ref(&self) -> &FutureSnapshot<NoLimitTracker> {
        unsafe { &*(self.inner as *mut FutureSnapshot<NoLimitTracker>) }
    }
}

#[repr(C)]
pub struct ProgressResult {
    pub kind: i32,
    pub result_json: *mut c_char,
    pub function_name: *mut c_char,
    pub os_function: *mut c_char,
    pub args_json: *mut c_char,
    pub kwargs_json: *mut c_char,
    pub call_id: u32,
    pub method_call: i32,
    pub snapshot: *mut SnapshotHandle,
    pub pending_call_ids_json: *mut c_char,
    pub future_snapshot: *mut FutureSnapshotHandle,
}

impl Default for ProgressResult {
    fn default() -> Self {
        Self {
            kind: MONTY_PROGRESS_COMPLETE,
            result_json: ptr::null_mut(),
            function_name: ptr::null_mut(),
            os_function: ptr::null_mut(),
            args_json: ptr::null_mut(),
            kwargs_json: ptr::null_mut(),
            call_id: 0,
            method_call: 0,
            snapshot: ptr::null_mut(),
            pending_call_ids_json: ptr::null_mut(),
            future_snapshot: ptr::null_mut(),
        }
    }
}

pub const MONTY_PROGRESS_COMPLETE: i32 = 0;
pub const MONTY_PROGRESS_FUNCTION_CALL: i32 = 1;
pub const MONTY_PROGRESS_OS_CALL: i32 = 2;
pub const MONTY_PROGRESS_RESOLVE_FUTURES: i32 = 3;

#[derive(Debug, Deserialize)]
struct FutureResultJson {
    call_id: u32,
    #[serde(default)]
    result: Option<Value>,
    #[serde(default)]
    error: Option<String>,
}

#[no_mangle]
pub unsafe extern "C" fn monty_run_new(
    code: *const c_char,
    script_name: *const c_char,
    input_names: *const *const c_char,
    ext_funcs: *const *const c_char,
    out: *mut *mut MontyRunHandle,
) -> MontyStatus {
    fn inner(
        code: *const c_char,
        script_name: *const c_char,
        input_names: *const *const c_char,
        ext_funcs: *const *const c_char,
        out: *mut *mut MontyRunHandle,
    ) -> FfiResult<()> {
        if out.is_null() {
            return Err(FfiError::NullPointer("out"));
        }
        let code = unsafe { read_required_str(code, "code") }?;
        let script_name = unsafe { read_required_str(script_name, "script_name") }?;
        let input_names = unsafe { read_string_array(input_names, "input_names")? };
        let ext_funcs = unsafe { read_string_array(ext_funcs, "ext_funcs")? };
        let runner = MontyRun::new(code, &script_name, input_names, ext_funcs)?;
        unsafe {
            *out = MontyRunHandle::new(runner);
        }
        Ok(())
    }

    match inner(code, script_name, input_names, ext_funcs, out) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_run_dump(
    run: *mut MontyRunHandle,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> MontyStatus {
    fn inner(
        run: *mut MontyRunHandle,
        out_bytes: *mut *mut u8,
        out_len: *mut usize,
    ) -> FfiResult<()> {
        let run = unsafe { run.as_ref().ok_or(FfiError::NullPointer("run"))? };
        let bytes = run.as_ref().dump()?;
        write_bytes(bytes, out_bytes, out_len)
    }

    match inner(run, out_bytes, out_len) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_run_load(
    bytes: *const u8,
    len: usize,
    out: *mut *mut MontyRunHandle,
) -> MontyStatus {
    fn inner(bytes: *const u8, len: usize, out: *mut *mut MontyRunHandle) -> FfiResult<()> {
        if out.is_null() {
            return Err(FfiError::NullPointer("out"));
        }
        if len > 0 && bytes.is_null() {
            return Err(FfiError::NullPointer("bytes"));
        }
        let slice = unsafe { slice::from_raw_parts(bytes, len) };
        let run = MontyRun::load(slice)?;
        unsafe {
            *out = MontyRunHandle::new(run);
        }
        Ok(())
    }

    match inner(bytes, len, out) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_run_free(run: *mut MontyRunHandle) {
    if !run.is_null() {
        let handle = Box::from_raw(run);
        drop(Box::from_raw(handle.inner as *mut MontyRun));
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_run_start(
    run: *mut MontyRunHandle,
    inputs_json: *const c_char,
    out: *mut ProgressResult,
) -> MontyStatus {
    fn inner(
        run: *mut MontyRunHandle,
        inputs_json: *const c_char,
        out: *mut ProgressResult,
    ) -> FfiResult<()> {
        if out.is_null() {
            return Err(FfiError::NullPointer("out"));
        }
        let run = unsafe { run.as_ref().ok_or(FfiError::NullPointer("run"))? };
        let inputs_json = unsafe {
            if inputs_json.is_null() {
                String::from("[]")
            } else {
                read_required_str(inputs_json, "inputs_json")?
            }
        };
        let inputs = decode_inputs(&inputs_json)?;
        let mut print = PrintWriter::Stdout;
        let progress = run
            .as_ref()
            .clone()
            .start(inputs, NoLimitTracker, &mut print)?;
        unsafe { write_progress_result(out, progress) }
    }

    match inner(run, inputs_json, out) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_progress_result_free_strings(result: *mut ProgressResult) {
    if let Some(result) = result.as_mut() {
        monty_free_string(result.result_json);
        monty_free_string(result.function_name);
        monty_free_string(result.os_function);
        monty_free_string(result.args_json);
        monty_free_string(result.kwargs_json);
        monty_free_string(result.pending_call_ids_json);
        result.result_json = ptr::null_mut();
        result.function_name = ptr::null_mut();
        result.os_function = ptr::null_mut();
        result.args_json = ptr::null_mut();
        result.kwargs_json = ptr::null_mut();
        result.pending_call_ids_json = ptr::null_mut();
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_snapshot_resume(
    snapshot: *mut SnapshotHandle,
    _call_id: u32,
    result_json: *const c_char,
    error_message: *const c_char,
    out: *mut ProgressResult,
) -> MontyStatus {
    fn inner(
        snapshot: *mut SnapshotHandle,
        result_json: *const c_char,
        error_message: *const c_char,
        out: *mut ProgressResult,
    ) -> FfiResult<()> {
        if out.is_null() {
            return Err(FfiError::NullPointer("out"));
        }
        if snapshot.is_null() {
            return Err(FfiError::NullPointer("snapshot"));
        }
        let resolution = if let Some(err) = unsafe { read_optional_str(error_message)? } {
            ExternalResult::Error(MontyException::new(ExcType::RuntimeError, Some(err)))
        } else if let Some(json) = unsafe { read_optional_str(result_json)? } {
            ExternalResult::Return(decode_object(&json)?)
        } else {
            ExternalResult::Future
        };
        let mut print = PrintWriter::Stdout;
        let snapshot = unsafe { Box::from_raw(snapshot) };
        let progress = snapshot.into_inner().run(resolution, &mut print)?;
        unsafe { write_progress_result(out, progress) }
    }

    match inner(snapshot, result_json, error_message, out) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_future_snapshot_resume(
    snapshot: *mut FutureSnapshotHandle,
    results_json: *const c_char,
    out: *mut ProgressResult,
) -> MontyStatus {
    fn inner(
        snapshot: *mut FutureSnapshotHandle,
        results_json: *const c_char,
        out: *mut ProgressResult,
    ) -> FfiResult<()> {
        if out.is_null() {
            return Err(FfiError::NullPointer("out"));
        }
        if snapshot.is_null() {
            return Err(FfiError::NullPointer("snapshot"));
        }
        let json = unsafe { read_required_str(results_json, "results_json") }?;
        let results = decode_future_results(&json)?;
        let mut print = PrintWriter::Stdout;
        let snapshot = unsafe { Box::from_raw(snapshot) };
        let progress = snapshot.into_inner().resume(results, &mut print)?;
        unsafe { write_progress_result(out, progress) }
    }

    match inner(snapshot, results_json, out) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_snapshot_dump(
    snapshot: *mut SnapshotHandle,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> MontyStatus {
    fn inner(
        snapshot: *mut SnapshotHandle,
        out_bytes: *mut *mut u8,
        out_len: *mut usize,
    ) -> FfiResult<()> {
        let snapshot = unsafe { snapshot.as_ref().ok_or(FfiError::NullPointer("snapshot"))? };
        let bytes = to_allocvec(snapshot.as_ref())?;
        write_bytes(bytes, out_bytes, out_len)
    }

    match inner(snapshot, out_bytes, out_len) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_snapshot_load(
    bytes: *const u8,
    len: usize,
    out: *mut *mut SnapshotHandle,
) -> MontyStatus {
    fn inner(bytes: *const u8, len: usize, out: *mut *mut SnapshotHandle) -> FfiResult<()> {
        if out.is_null() {
            return Err(FfiError::NullPointer("out"));
        }
        if len > 0 && bytes.is_null() {
            return Err(FfiError::NullPointer("bytes"));
        }
        let slice = unsafe { slice::from_raw_parts(bytes, len) };
        let snapshot: Snapshot<NoLimitTracker> = from_bytes(slice)?;
        unsafe {
            *out = SnapshotHandle::new(snapshot);
        }
        Ok(())
    }

    match inner(bytes, len, out) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_future_snapshot_dump(
    snapshot: *mut FutureSnapshotHandle,
    out_bytes: *mut *mut u8,
    out_len: *mut usize,
) -> MontyStatus {
    fn inner(
        snapshot: *mut FutureSnapshotHandle,
        out_bytes: *mut *mut u8,
        out_len: *mut usize,
    ) -> FfiResult<()> {
        let snapshot = unsafe { snapshot.as_ref().ok_or(FfiError::NullPointer("snapshot"))? };
        let bytes = to_allocvec(snapshot.as_ref())?;
        write_bytes(bytes, out_bytes, out_len)
    }

    match inner(snapshot, out_bytes, out_len) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_future_snapshot_load(
    bytes: *const u8,
    len: usize,
    out: *mut *mut FutureSnapshotHandle,
) -> MontyStatus {
    fn inner(bytes: *const u8, len: usize, out: *mut *mut FutureSnapshotHandle) -> FfiResult<()> {
        if out.is_null() {
            return Err(FfiError::NullPointer("out"));
        }
        if len > 0 && bytes.is_null() {
            return Err(FfiError::NullPointer("bytes"));
        }
        let slice = unsafe { slice::from_raw_parts(bytes, len) };
        let snapshot: FutureSnapshot<NoLimitTracker> = from_bytes(slice)?;
        unsafe {
            *out = FutureSnapshotHandle::new(snapshot);
        }
        Ok(())
    }

    match inner(bytes, len, out) {
        Ok(()) => MontyStatus::success(),
        Err(err) => MontyStatus::from_error(err),
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_snapshot_free(snapshot: *mut SnapshotHandle) {
    if !snapshot.is_null() {
        let handle = Box::from_raw(snapshot);
        drop(Box::from_raw(handle.inner as *mut Snapshot<NoLimitTracker>));
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_future_snapshot_free(snapshot: *mut FutureSnapshotHandle) {
    if !snapshot.is_null() {
        let handle = Box::from_raw(snapshot);
        drop(Box::from_raw(
            handle.inner as *mut FutureSnapshot<NoLimitTracker>,
        ));
    }
}

#[no_mangle]
pub unsafe extern "C" fn monty_free_bytes(ptr: *mut u8, len: usize) {
    if !ptr.is_null() {
        drop(Vec::from_raw_parts(ptr, len, len));
    }
}

fn write_bytes(bytes: Vec<u8>, out_bytes: *mut *mut u8, out_len: *mut usize) -> FfiResult<()> {
    if out_bytes.is_null() {
        return Err(FfiError::NullPointer("out_bytes"));
    }
    if out_len.is_null() {
        return Err(FfiError::NullPointer("out_len"));
    }
    let mut boxed = bytes.into_boxed_slice();
    let len = boxed.len();
    let ptr = boxed.as_mut_ptr();
    std::mem::forget(boxed);
    unsafe {
        *out_bytes = ptr;
        *out_len = len;
    }
    Ok(())
}

unsafe fn read_string_array(
    ptr: *const *const c_char,
    field: &'static str,
) -> FfiResult<Vec<String>> {
    if ptr.is_null() {
        return Ok(Vec::new());
    }
    let mut values = Vec::new();
    let mut index = 0;
    loop {
        let current = unsafe { *ptr.add(index) };
        if current.is_null() {
            break;
        }
        values.push(unsafe { read_required_str(current, field)? });
        index += 1;
    }
    Ok(values)
}

fn decode_future_results(json: &str) -> FfiResult<Vec<(u32, ExternalResult)>> {
    let raw: Vec<FutureResultJson> = serde_json::from_str(json)?;
    raw.into_iter()
        .map(|entry| {
            if let Some(err) = entry.error.filter(|s| !s.is_empty()) {
                return Ok((
                    entry.call_id,
                    ExternalResult::Error(MontyException::new(ExcType::RuntimeError, Some(err))),
                ));
            }
            if let Some(value) = entry.result {
                let object = decode_value(value)?;
                return Ok((entry.call_id, ExternalResult::Return(object)));
            }
            Ok((entry.call_id, ExternalResult::Future))
        })
        .collect()
}

unsafe fn write_progress_result(
    out: *mut ProgressResult,
    progress: RunProgress<NoLimitTracker>,
) -> FfiResult<()> {
    let result = out.as_mut().ok_or(FfiError::NullPointer("out"))?;
    *result = ProgressResult::default();
    match progress {
        RunProgress::Complete(value) => {
            result.kind = MONTY_PROGRESS_COMPLETE;
            let json = encode_object(&value)?;
            result.result_json = to_c_string(json, "result_json")?;
        }
        RunProgress::FunctionCall {
            function_name,
            args,
            kwargs,
            call_id,
            method_call,
            state,
        } => {
            result.kind = MONTY_PROGRESS_FUNCTION_CALL;
            result.function_name = to_c_string(function_name, "function_name")?;
            result.args_json = to_c_string(encode_objects(&args)?, "args_json")?;
            result.kwargs_json = to_c_string(encode_kwargs(&kwargs)?, "kwargs_json")?;
            result.call_id = call_id;
            result.method_call = method_call as i32;
            result.snapshot = SnapshotHandle::new(state);
        }
        RunProgress::OsCall {
            function,
            args,
            kwargs,
            call_id,
            state,
        } => {
            result.kind = MONTY_PROGRESS_OS_CALL;
            result.os_function = to_c_string(function.to_string(), "os_function")?;
            result.args_json = to_c_string(encode_objects(&args)?, "args_json")?;
            result.kwargs_json = to_c_string(encode_kwargs(&kwargs)?, "kwargs_json")?;
            result.call_id = call_id;
            result.snapshot = SnapshotHandle::new(state);
        }
        RunProgress::ResolveFutures(state) => {
            result.kind = MONTY_PROGRESS_RESOLVE_FUTURES;
            result.pending_call_ids_json = to_c_string(
                encode_u32_slice(state.pending_call_ids())?,
                "pending_call_ids",
            )?;
            result.future_snapshot = FutureSnapshotHandle::new(state);
        }
    }
    Ok(())
}
