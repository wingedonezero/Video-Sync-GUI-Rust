//! GPU backend — 1:1 port of `correlation/gpu_backend.py`.

use std::sync::Mutex;

use once_cell::sync::Lazy;
use tch::{Device, Tensor};

static DEVICE: Lazy<Mutex<Option<Device>>> = Lazy::new(|| Mutex::new(None));

/// Get the torch device — `get_device`
pub fn get_device() -> Device {
    let mut guard = DEVICE.lock().unwrap();
    if let Some(device) = *guard {
        return device;
    }

    let device = if tch::Cuda::is_available() {
        tracing::info!("GPU correlation backend: CUDA/ROCm available");
        Device::Cuda(0)
    } else {
        tracing::info!("GPU correlation backend: CPU fallback (no CUDA/ROCm)");
        Device::Cpu
    };

    *guard = Some(device);
    device
}

/// Convert f32 slice to torch tensor on device — `to_torch`
pub fn to_torch(arr: &[f32], device: Device) -> Tensor {
    Tensor::from_slice(arr).to(device)
}

/// Release GPU resources — `cleanup_gpu`
pub fn cleanup_gpu() {
    if tch::Cuda::is_available() {
        tch::Cuda::synchronize(0);
    }
}
