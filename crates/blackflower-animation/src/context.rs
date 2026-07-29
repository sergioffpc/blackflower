use std::sync::{Arc, Weak};

use crate::Error;
use crate::asset::Animation;
use crate::error::map_native_failure;
use crate::ffi::{self, Status};

/// Reusable decompression cache for animation sampling.
#[derive(Debug)]
pub struct SamplingContext {
    pub(crate) pointer: ffi::ContextPtr,
    max_tracks: usize,
    cached_animation: Weak<()>,
}

impl SamplingContext {
    /// Allocate a context able to sample clips with up to `max_tracks` tracks.
    pub fn new(max_tracks: usize) -> Result<Self, Error> {
        let native_capacity = u32::try_from(max_tracks)
            .map_err(|_error| Error::InvalidContextCapacity(max_tracks))?;
        if native_capacity == 0 {
            return Err(Error::InvalidContextCapacity(max_tracks));
        }
        let pointer = ffi::create_context(native_capacity).map_err(|status| match status {
            Status::InvalidArgument => Error::InvalidContextCapacity(max_tracks),
            Status::InvalidArchive
            | Status::OutOfMemory
            | Status::Incompatible
            | Status::JobFailed
            | Status::IndexOutOfRange
            | Status::NativeFailure
            | Status::ContractViolation => map_native_failure(status),
        })?;
        let max_tracks = usize::try_from(ffi::context_max_tracks(pointer))
            .map_err(|_error| Error::NativeContract)?;
        Ok(Self {
            pointer,
            max_tracks,
            cached_animation: Weak::new(),
        })
    }

    /// Return the actual track capacity, rounded for ozz's SoA storage.
    #[must_use]
    pub const fn max_tracks(&self) -> usize {
        self.max_tracks
    }

    pub(crate) fn prepare(&mut self, animation: &Animation) {
        let identity = Arc::downgrade(&animation.identity);
        if !Weak::ptr_eq(&self.cached_animation, &identity) {
            ffi::invalidate_context(self.pointer);
            self.cached_animation = identity;
        }
    }
}

impl Drop for SamplingContext {
    fn drop(&mut self) {
        ffi::destroy_context(self.pointer);
    }
}
