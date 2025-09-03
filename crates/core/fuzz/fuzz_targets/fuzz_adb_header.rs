#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() != 24 { return; }
    let mut arr = [0u8;24];
    arr.copy_from_slice(&data[..24]);
    // Try decode; ignore errors.
    let _ = miassistant_core::adb::decode_header(&arr);
});
