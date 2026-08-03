use crate::ffi::NativeContext;
use crate::{Backend, Error};

/// Optimized Flow context backed by renderer-owned resources and commands.
pub struct Context<B: Backend> {
    native: NativeContext<B>,
}

impl<B: Backend> Context<B> {
    /// Wraps a renderer backend with NVIDIA's `NvFlowContextOpt` scheduler.
    pub fn new(backend: B) -> Result<Self, Error> {
        NativeContext::new(backend).map(|native| Self { native })
    }

    /// Keeps otherwise-unused pooled resources alive for at least `frames` frames.
    pub fn set_min_resource_lifetime(&mut self, frames: u64) -> Result<(), Error> {
        self.native.set_min_resource_lifetime(frames)
    }

    /// Flushes deferred resource and pass operations into the renderer backend.
    pub fn flush(&mut self) -> Result<(), Error> {
        self.native.flush()
    }

    /// Exercises upload allocation, mapping, unmapping, retirement, and flush.
    ///
    /// This is the minimum backend gate before Flow Grid pipelines are created.
    pub fn validate_upload_path(&mut self, size_in_bytes: u64) -> Result<(), Error> {
        if size_in_bytes == 0 {
            return Err(Error::InvalidArgument);
        }
        self.native.validate_upload(size_in_bytes)
    }

    /// Read-only access for renderer telemetry and test inspection.
    #[must_use]
    pub fn backend(&self) -> &B {
        self.native.backend()
    }

    /// Mutable access for renderer frame lifecycle and command submission.
    pub fn backend_mut(&mut self) -> &mut B {
        self.native.backend_mut()
    }
}
