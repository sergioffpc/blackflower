use std::sync::Arc;

use crate::asset::{BakedDataIdentifier, BakedLayer, ProbeBatch};
use crate::error::Error;
use crate::ffi;
use crate::hrtf::{Context, ContextInner};
use crate::scene::Scene;

/// Oriented volume transform used by Steam Audio probe generation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProbeVolumeTransform([[f32; 4]; 4]);

impl ProbeVolumeTransform {
    /// Create a finite affine row-major transform whose first three basis
    /// vectors have non-zero length.
    pub fn new(rows: [[f32; 4]; 4]) -> Result<Self, Error> {
        let finite = rows.into_iter().flatten().all(f32::is_finite);
        let affine = rows[3]
            .into_iter()
            .zip([0.0, 0.0, 0.0, 1.0])
            .all(|(value, expected)| (value - expected).abs() <= f32::EPSILON);
        let non_degenerate = (0..3).all(|column| {
            let length_squared = rows[0][column].mul_add(
                rows[0][column],
                rows[1][column].mul_add(rows[1][column], rows[2][column] * rows[2][column]),
            );
            length_squared.is_finite() && length_squared > f32::MIN_POSITIVE
        });
        if finite && affine && non_degenerate {
            Ok(Self(rows))
        } else {
            Err(Error::InvalidProbeSettings)
        }
    }

    pub(crate) const fn rows(self) -> [[f32; 4]; 4] {
        self.0
    }
}

/// Quality settings for base reflections and parametric reverb baking.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReflectionsBakeSettings {
    pub(crate) num_rays: u32,
    pub(crate) num_diffuse_samples: u32,
    pub(crate) num_bounces: u32,
    pub(crate) simulated_duration: f32,
    pub(crate) saved_duration: f32,
    pub(crate) order: u32,
    pub(crate) num_threads: u32,
    pub(crate) ray_batch_size: u32,
    pub(crate) irradiance_min_distance: f32,
    pub(crate) bake_batch_size: u32,
}

impl ReflectionsBakeSettings {
    /// Validate the complete Steam Audio reflection-bake recipe.
    #[allow(
        clippy::too_many_arguments,
        reason = "the constructor mirrors the explicit native bake contract"
    )]
    pub fn new(
        num_rays: u32,
        num_diffuse_samples: u32,
        num_bounces: u32,
        simulated_duration: f32,
        saved_duration: f32,
        order: u32,
        num_threads: u32,
        ray_batch_size: u32,
        irradiance_min_distance: f32,
        bake_batch_size: u32,
    ) -> Result<Self, Error> {
        let positive_counts = [
            num_rays,
            num_diffuse_samples,
            num_bounces,
            num_threads,
            ray_batch_size,
            bake_batch_size,
        ]
        .into_iter()
        .all(|value| value > 0 && i32::try_from(value).is_ok());
        let durations = simulated_duration.is_finite()
            && saved_duration.is_finite()
            && simulated_duration > 0.0
            && saved_duration > 0.0
            && saved_duration <= simulated_duration;
        let distance = irradiance_min_distance.is_finite() && irradiance_min_distance > 0.0;
        if positive_counts && durations && distance && (1..=3).contains(&order) {
            Ok(Self {
                num_rays,
                num_diffuse_samples,
                num_bounces,
                simulated_duration,
                saved_duration,
                order,
                num_threads,
                ray_batch_size,
                irradiance_min_distance,
                bake_batch_size,
            })
        } else {
            Err(Error::InvalidProbeSettings)
        }
    }
}

/// Quality settings for probe-to-probe pathing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathBakeSettings {
    pub(crate) num_samples: u32,
    pub(crate) radius: f32,
    pub(crate) threshold: f32,
    pub(crate) visibility_range: f32,
    pub(crate) path_range: f32,
    pub(crate) num_threads: u32,
}

impl PathBakeSettings {
    /// Validate the complete Steam Audio path-bake recipe.
    pub fn new(
        num_samples: u32,
        radius: f32,
        threshold: f32,
        visibility_range: f32,
        path_range: f32,
        num_threads: u32,
    ) -> Result<Self, Error> {
        let counts = num_samples > 0
            && num_threads > 0
            && i32::try_from(num_samples).is_ok()
            && i32::try_from(num_threads).is_ok();
        let positive = [radius, visibility_range, path_range]
            .into_iter()
            .all(|value| value.is_finite() && value > 0.0);
        if counts && positive && threshold.is_finite() && (0.0..=1.0).contains(&threshold) {
            Ok(Self {
                num_samples,
                radius,
                threshold,
                visibility_range,
                path_range,
                num_threads,
            })
        } else {
            Err(Error::InvalidProbeSettings)
        }
    }
}

struct ProbeBatchInner {
    _context: Arc<ContextInner>,
    pointer: ffi::ProbeBatchPtr,
}

impl Drop for ProbeBatchInner {
    fn drop(&mut self) {
        ffi::destroy_probe_batch(self.pointer);
    }
}

/// Native Steam Audio probe batch loaded outside the real-time callback.
pub struct LoadedProbeBatch {
    _inner: ProbeBatchInner,
    probe_count: usize,
}

impl LoadedProbeBatch {
    /// Number of probes in the native batch.
    #[must_use]
    pub const fn probe_count(&self) -> usize {
        self.probe_count
    }
}

impl Context {
    /// Generate uniform-floor probes, bake base reflections/reverb and dynamic
    /// pathing, and return a complete `.bfacprb` asset.
    #[allow(
        clippy::too_many_arguments,
        reason = "placement and two independent quality profiles are explicit cooker inputs"
    )]
    pub fn bake_uniform_floor_probe_batch(
        &mut self,
        scene: &Scene,
        zone: impl Into<String>,
        volume: ProbeVolumeTransform,
        spacing_meters: f32,
        height_meters: f32,
        reflections: ReflectionsBakeSettings,
        pathing: PathBakeSettings,
    ) -> Result<ProbeBatch, Error> {
        if !Arc::ptr_eq(&self.inner, &scene.inner().context) {
            return Err(Error::WrongAcousticContext);
        }
        if !spacing_meters.is_finite()
            || spacing_meters <= 0.0
            || !height_meters.is_finite()
            || height_meters <= 0.0
        {
            return Err(Error::InvalidProbeSettings);
        }
        let (pointer, probes) = ffi::generate_uniform_floor_probes(
            self.inner.pointer,
            scene.inner().pointer,
            volume,
            spacing_meters,
            height_meters,
        )
        .map_err(|status| Error::from_status("iplProbeArrayGenerateProbes", status))?;
        let native = ProbeBatchInner {
            _context: Arc::clone(&self.inner),
            pointer,
        };
        if probes.is_empty() {
            return Err(Error::InvalidProbeSettings);
        }

        let layers = self.bake_probe_layers(scene, &native, reflections, pathing)?;
        let serialized = ffi::save_probe_batch(self.inner.pointer, native.pointer)
            .map_err(|status| Error::from_status("iplProbeBatchSave", status))?;
        ProbeBatch::encode(zone.into(), probes, layers, serialized)
    }

    fn bake_probe_layers(
        &self,
        scene: &Scene,
        native: &ProbeBatchInner,
        reflections: ReflectionsBakeSettings,
        pathing: PathBakeSettings,
    ) -> Result<Vec<BakedLayer>, Error> {
        let reverb = BakedDataIdentifier::reverb()?;
        ffi::bake_reflections(
            self.inner.pointer,
            scene.inner().pointer,
            scene.ray_tracer_backend(),
            native.pointer,
            reverb,
            reflections,
        );
        let reverb_size = ffi::probe_batch_data_size(native.pointer, reverb);
        let path = BakedDataIdentifier::dynamic_pathing()?;
        ffi::bake_pathing(
            self.inner.pointer,
            scene.inner().pointer,
            native.pointer,
            path,
            pathing,
        );
        let path_size = ffi::probe_batch_data_size(native.pointer, path);
        if reverb_size == 0 || path_size == 0 {
            return Err(Error::NativeContract {
                operation: "Steam Audio bake",
            });
        }
        Ok(vec![
            BakedLayer::new(reverb, baked_data_size(reverb_size)?),
            BakedLayer::new(path, baked_data_size(path_size)?),
        ])
    }

    /// Load a parsed `.bfacprb` into a native Steam Audio probe batch.
    pub fn load_probe_batch(&mut self, asset: &ProbeBatch) -> Result<LoadedProbeBatch, Error> {
        let pointer = ffi::load_probe_batch(self.inner.pointer, asset.serialized())
            .map_err(|status| Error::from_status("iplProbeBatchLoad", status))?;
        let native = ProbeBatchInner {
            _context: Arc::clone(&self.inner),
            pointer,
        };
        let native_count =
            ffi::probe_batch_count(native.pointer).map_err(|()| Error::NativeContract {
                operation: "iplProbeBatchGetNumProbes",
            })?;
        if native_count != asset.probes().len() {
            return Err(Error::NativeContract {
                operation: "iplProbeBatchLoad probe count",
            });
        }
        for layer in asset.layers() {
            let expected =
                usize::try_from(layer.byte_len()).map_err(|_error| Error::NativeContract {
                    operation: "iplProbeBatchLoad data size",
                })?;
            if ffi::probe_batch_data_size(native.pointer, layer.identifier()) != expected {
                return Err(Error::NativeContract {
                    operation: "iplProbeBatchLoad data size",
                });
            }
        }
        Ok(LoadedProbeBatch {
            _inner: native,
            probe_count: native_count,
        })
    }
}

fn baked_data_size(size: usize) -> Result<u64, Error> {
    u64::try_from(size).map_err(|_error| Error::NativeContract {
        operation: "iplProbeBatchGetDataSize",
    })
}
