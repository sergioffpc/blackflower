use std::cell::Cell;
use std::marker::PhantomData;
use std::sync::Arc;

use crate::error::Error;
use crate::ffi;
use crate::types::{AudioSettings, BinauralParams, TailState};

pub(crate) struct ContextInner {
    pub(crate) pointer: ffi::ContextPtr,
}

impl Drop for ContextInner {
    fn drop(&mut self) {
        ffi::destroy_context(self.pointer);
    }
}

/// Owning handle for one statically linked Steam Audio context.
pub struct Context {
    pub(crate) inner: Arc<ContextInner>,
}

impl Context {
    /// Create a context using the statically linked Steam Audio SDK.
    pub fn new() -> Result<Self, Error> {
        let pointer = ffi::create_context()
            .map_err(|status| Error::from_status("iplContextCreate", status))?;
        Ok(Self {
            inner: Arc::new(ContextInner { pointer }),
        })
    }

    /// Create Steam Audio's built-in HRTF for one audio configuration.
    pub fn create_default_hrtf(&mut self, audio: AudioSettings) -> Result<Hrtf, Error> {
        let pointer = ffi::create_default_hrtf(self.inner.pointer, audio)
            .map_err(|status| Error::from_status("iplHRTFCreate", status))?;
        Ok(Hrtf {
            inner: Arc::new(HrtfInner {
                context: Arc::clone(&self.inner),
                pointer,
                audio,
            }),
        })
    }

    /// Create a stateful binaural renderer for one point source.
    pub fn create_binaural_effect(&mut self, hrtf: &Hrtf) -> Result<BinauralEffect, Error> {
        if !Arc::ptr_eq(&self.inner, &hrtf.inner.context) {
            return Err(Error::WrongContext);
        }
        let pointer =
            ffi::create_binaural_effect(self.inner.pointer, hrtf.inner.pointer, hrtf.inner.audio)
                .map_err(|status| Error::from_status("iplBinauralEffectCreate", status))?;
        Ok(BinauralEffect {
            pointer,
            hrtf: Arc::clone(&hrtf.inner),
            not_sync: PhantomData,
        })
    }

    /// Create a mutable scene using Steam Audio's built-in ray tracer.
    pub fn create_scene(&mut self) -> Result<crate::Scene, Error> {
        crate::Scene::new(Arc::clone(&self.inner))
    }
}

/// Reference-counted Steam Audio HRTF tied to one [`AudioSettings`] value.
pub struct Hrtf {
    inner: Arc<HrtfInner>,
}

struct HrtfInner {
    context: Arc<ContextInner>,
    pointer: ffi::HrtfPtr,
    audio: AudioSettings,
}

impl Drop for HrtfInner {
    fn drop(&mut self) {
        ffi::destroy_hrtf(self.pointer);
    }
}

impl Hrtf {
    /// Signal-processing settings used to create this HRTF.
    #[must_use]
    pub fn audio_settings(&self) -> AudioSettings {
        self.inner.audio
    }
}

/// Stateful mono-to-stereo HRTF renderer for one point source.
///
/// The effect is `Send` so it can be moved into an audio callback, but it is
/// deliberately not `Sync`: processing one effect concurrently would race its
/// native filter state.
pub struct BinauralEffect {
    pointer: ffi::BinauralEffectPtr,
    hrtf: Arc<HrtfInner>,
    not_sync: PhantomData<Cell<()>>,
}

impl BinauralEffect {
    /// Process one fixed-size mono frame into deinterleaved stereo output.
    pub fn process_mono(
        &mut self,
        params: BinauralParams,
        input: &[f32],
        output_left: &mut [f32],
        output_right: &mut [f32],
    ) -> Result<TailState, Error> {
        self.validate_frame("input", input.len())?;
        self.validate_frame("left output", output_left.len())?;
        self.validate_frame("right output", output_right.len())?;
        ffi::apply_binaural_effect(
            self.pointer,
            self.hrtf.pointer,
            self.hrtf.audio,
            params,
            input,
            output_left,
            output_right,
        )
        .map_err(|status| Error::from_status("iplBinauralEffectApply", status))
    }

    /// Retrieve one output frame after source input has ended.
    pub fn get_tail(
        &mut self,
        output_left: &mut [f32],
        output_right: &mut [f32],
    ) -> Result<TailState, Error> {
        self.validate_frame("left output", output_left.len())?;
        self.validate_frame("right output", output_right.len())?;
        ffi::get_binaural_tail(self.pointer, self.hrtf.audio, output_left, output_right)
            .map_err(|status| Error::from_status("iplBinauralEffectGetTail", status))
    }

    /// Return the number of tail samples currently buffered by the effect.
    pub fn tail_size(&self) -> Result<usize, Error> {
        usize::try_from(ffi::binaural_tail_size(self.pointer)).map_err(|_error| {
            Error::NativeContract {
                operation: "iplBinauralEffectGetTailSize",
            }
        })
    }

    /// Clear the effect's internal filter history.
    pub fn reset(&mut self) {
        ffi::reset_binaural_effect(self.pointer);
    }

    /// Fixed frame configuration used by this effect.
    #[must_use]
    pub fn audio_settings(&self) -> AudioSettings {
        self.hrtf.audio
    }

    fn validate_frame(&self, buffer: &'static str, actual: usize) -> Result<(), Error> {
        let expected = self.hrtf.audio.frame_len();
        if actual == expected {
            Ok(())
        } else {
            Err(Error::FrameLength {
                buffer,
                expected,
                actual,
            })
        }
    }
}

impl Drop for BinauralEffect {
    fn drop(&mut self) {
        ffi::destroy_binaural_effect(self.pointer);
    }
}
