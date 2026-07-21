//! Narrow dynamic C ABI around C++ libsumo.
//!
//! Safety: the loaded library is accepted only when it exports ABI version 1.
//! Function pointers are copied while `Library` is retained for the whole engine
//! lifetime. Every pointer argument refers to a live bounded buffer for the full
//! call, and returned text is copied before another native call can mutate it.

#![allow(unsafe_code)]

use libloading::Library;
use roadsim_worker_protocol::{EngineIdentity, MAX_STEPS_PER_REQUEST};
use std::{
    ffi::{CStr, CString, c_char},
    path::Path,
};

const BRIDGE_ABI_VERSION: u32 = 1;
const TEXT_CAPACITY: usize = 512;

type AbiVersionFn = unsafe extern "C" fn() -> u32;
type TextFn = unsafe extern "C" fn(*mut c_char, usize) -> i32;
type StartFn = unsafe extern "C" fn(*const c_char, u64, u32, *mut c_char, usize) -> i32;
type StepFn = unsafe extern "C" fn(u32, *mut u64, *mut c_char, usize) -> i32;
type CloseFn = unsafe extern "C" fn(*mut c_char, usize) -> i32;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NativeErrorCode {
    LoadFailed,
    AbiMismatch,
    IdentityInvalid,
    StartFailed,
    StepFailed,
    CloseFailed,
}

pub struct NativeEngine {
    _library: Library,
    version: TextFn,
    revision: TextFn,
    start: StartFn,
    step: StepFn,
    close: CloseFn,
    active: bool,
}

impl NativeEngine {
    pub fn load(path: &Path) -> Result<Self, NativeErrorCode> {
        // SAFETY: symbol types are the reviewed roadsim_sumo_bridge ABI. The
        // version function is checked before any stateful function is called,
        // and the Library is stored so copied function pointers remain valid.
        unsafe {
            let library = Library::new(path).map_err(|_| NativeErrorCode::LoadFailed)?;
            let abi = *library
                .get::<AbiVersionFn>(b"roadsim_sumo_bridge_abi\0")
                .map_err(|_| NativeErrorCode::LoadFailed)?;
            if abi() != BRIDGE_ABI_VERSION {
                return Err(NativeErrorCode::AbiMismatch);
            }
            let version = *library
                .get::<TextFn>(b"roadsim_sumo_engine_version\0")
                .map_err(|_| NativeErrorCode::LoadFailed)?;
            let revision = *library
                .get::<TextFn>(b"roadsim_sumo_engine_revision\0")
                .map_err(|_| NativeErrorCode::LoadFailed)?;
            let start = *library
                .get::<StartFn>(b"roadsim_sumo_start\0")
                .map_err(|_| NativeErrorCode::LoadFailed)?;
            let step = *library
                .get::<StepFn>(b"roadsim_sumo_step\0")
                .map_err(|_| NativeErrorCode::LoadFailed)?;
            let close = *library
                .get::<CloseFn>(b"roadsim_sumo_close\0")
                .map_err(|_| NativeErrorCode::LoadFailed)?;
            Ok(Self {
                _library: library,
                version,
                revision,
                start,
                step,
                close,
                active: false,
            })
        }
    }

    pub fn identity(&self) -> Result<EngineIdentity, NativeErrorCode> {
        let version = call_text(self.version).map_err(|_| NativeErrorCode::IdentityInvalid)?;
        let revision = call_text(self.revision).map_err(|_| NativeErrorCode::IdentityInvalid)?;
        EngineIdentity::new("eclipse.sumo", version, revision)
            .map_err(|_| NativeErrorCode::IdentityInvalid)
    }

    pub fn start(
        &mut self,
        bundle_path: &str,
        root_seed: u64,
        step_length_ms: u32,
    ) -> Result<(), NativeErrorCode> {
        let bundle_path = CString::new(bundle_path).map_err(|_| NativeErrorCode::StartFailed)?;
        let mut error = [0_u8; TEXT_CAPACITY];
        // SAFETY: CString is NUL-terminated; error is writable for its declared
        // capacity; the ABI does not retain either pointer after returning.
        let result = unsafe {
            (self.start)(
                bundle_path.as_ptr(),
                root_seed,
                step_length_ms,
                error.as_mut_ptr().cast::<c_char>(),
                error.len(),
            )
        };
        if result != 0 {
            return Err(NativeErrorCode::StartFailed);
        }
        self.active = true;
        Ok(())
    }

    pub fn step(&mut self, steps: u32) -> Result<u64, NativeErrorCode> {
        if steps == 0 || steps > MAX_STEPS_PER_REQUEST {
            return Err(NativeErrorCode::StepFailed);
        }
        let mut tick = 0_u64;
        let mut error = [0_u8; TEXT_CAPACITY];
        // SAFETY: both output pointers are valid and writable for the call; the
        // ABI does not retain them.
        let result = unsafe {
            (self.step)(
                steps,
                &mut tick,
                error.as_mut_ptr().cast::<c_char>(),
                error.len(),
            )
        };
        if result != 0 {
            return Err(NativeErrorCode::StepFailed);
        }
        Ok(tick)
    }

    pub fn close(&mut self) -> Result<(), NativeErrorCode> {
        let mut error = [0_u8; TEXT_CAPACITY];
        // SAFETY: error is a valid bounded writable buffer for this call.
        let result = unsafe { (self.close)(error.as_mut_ptr().cast::<c_char>(), error.len()) };
        if result != 0 {
            return Err(NativeErrorCode::CloseFailed);
        }
        self.active = false;
        Ok(())
    }
}

impl Drop for NativeEngine {
    fn drop(&mut self) {
        if self.active {
            let _ = self.close();
        }
    }
}

fn call_text(function: TextFn) -> Result<String, ()> {
    let mut buffer = [0_u8; TEXT_CAPACITY];
    // SAFETY: buffer is writable for the declared capacity. A successful ABI
    // call guarantees NUL termination within that capacity.
    if unsafe { function(buffer.as_mut_ptr().cast::<c_char>(), buffer.len()) } != 0 {
        return Err(());
    }
    let text = CStr::from_bytes_until_nul(&buffer)
        .map_err(|_| ())?
        .to_str()
        .map_err(|_| ())?;
    Ok(text.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_bridge_fails_without_exposing_a_native_handle() {
        let result = NativeEngine::load(Path::new("definitely-missing-roadsim-bridge"));
        assert!(matches!(result, Err(NativeErrorCode::LoadFailed)));
    }
}
