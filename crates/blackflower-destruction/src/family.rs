use std::marker::PhantomData;
use std::rc::Rc;

use glam::Vec3A;

use crate::ffi;
use crate::{
    ActorId, Asset, Error, ForceMode, FractureCommand, FractureEvent, GraphNodeId, StressSettings,
    StressStats,
};

/// Mutable instance of one immutable destruction asset.
pub struct Family<'asset> {
    pointer: ffi::FamilyPointer,
    asset: &'asset Asset,
    not_send_or_sync: PhantomData<Rc<()>>,
}

impl<'asset> Family<'asset> {
    /// Creates the first actor with uniform bond and lower-support chunk health.
    pub fn new(
        asset: &'asset Asset,
        initial_bond_health: f32,
        initial_chunk_health: f32,
    ) -> Result<Self, Error> {
        validate_health(initial_bond_health)?;
        validate_health(initial_chunk_health)?;
        let pointer = ffi::create_family(asset.pointer, initial_bond_health, initial_chunk_health)?;
        Ok(Self {
            pointer,
            asset,
            not_send_or_sync: PhantomData,
        })
    }

    /// Returns active actor identifiers in ascending family-local index order.
    pub fn actors(&self) -> Result<Vec<ActorId>, Error> {
        ffi::actor_ids(self.pointer)
    }

    /// Returns the visible asset chunks represented by an active actor.
    pub fn visible_chunks(&self, actor: ActorId) -> Result<Vec<u32>, Error> {
        ffi::visible_chunks(self.pointer, actor, self.asset.chunk_count)
    }

    /// Applies direct domain damage and returns the resulting health events.
    pub fn apply_fracture(
        &mut self,
        actor: ActorId,
        commands: &[FractureCommand],
    ) -> Result<Vec<FractureEvent>, Error> {
        if commands.iter().any(|command| match command {
            FractureCommand::Bond { damage, .. } | FractureCommand::Chunk { damage, .. } => {
                !damage.is_finite() || *damage <= 0.0
            }
        }) {
            return Err(Error::InvalidHealth);
        }
        if commands.iter().any(|command| match command {
            FractureCommand::Bond { first, second, .. } => {
                first == second
                    || first.get() >= self.asset.graph_node_count
                    || second.get() >= self.asset.graph_node_count
            }
            FractureCommand::Chunk { chunk_index, .. } => *chunk_index >= self.asset.chunk_count,
        }) {
            return Err(Error::InvalidFractureTarget);
        }
        let native = commands
            .iter()
            .copied()
            .map(ffi::NativeFracture::from)
            .collect::<Vec<_>>();
        ffi::apply_fracture(self.pointer, actor, &native, self.maximum_event_count())
    }

    /// Splits a damaged actor and returns replacement actor identifiers.
    pub fn split_actor(&mut self, actor: ActorId) -> Result<Vec<ActorId>, Error> {
        ffi::split_actor(self.pointer, actor, self.asset.chunk_count)
    }

    /// Enables `NvBlastExtStress` and initializes node mass from chunk volume.
    pub fn enable_stress(&mut self, settings: StressSettings, density: f32) -> Result<(), Error> {
        validate_stress(settings, density)?;
        ffi::enable_stress(self.pointer, settings, density)
    }

    /// Adds force or acceleration to one support-graph node for the next update.
    pub fn add_stress_force(
        &mut self,
        node: GraphNodeId,
        vector: Vec3A,
        mode: ForceMode,
    ) -> Result<(), Error> {
        if !vector.is_finite() {
            return Err(Error::InvalidStressSettings);
        }
        if node.get() >= self.asset.graph_node_count {
            return Err(Error::GraphNodeNotFound);
        }
        ffi::stress_add_force(self.pointer, node, vector, mode)
    }

    /// Advances the stress solver after all forces for the visual/simulation tick were submitted.
    pub fn update_stress(&mut self) -> Result<(), Error> {
        ffi::stress_update(self.pointer)
    }

    /// Converts overstressed bonds for one actor into Blast fracture events.
    pub fn apply_stress(&mut self, actor: ActorId) -> Result<Vec<FractureEvent>, Error> {
        ffi::apply_stress(self.pointer, actor, self.maximum_event_count())
    }

    /// Returns telemetry for the enabled stress solver.
    pub fn stress_stats(&self) -> Result<StressStats, Error> {
        ffi::stress_stats(self.pointer)
    }

    fn maximum_event_count(&self) -> u32 {
        self.asset.chunk_count.saturating_add(self.asset.bond_count)
    }
}

impl Drop for Family<'_> {
    fn drop(&mut self) {
        ffi::destroy_family(self.pointer);
    }
}

fn validate_health(value: f32) -> Result<(), Error> {
    if value.is_finite() && value > 0.0 {
        Ok(())
    } else {
        Err(Error::InvalidHealth)
    }
}

fn validate_stress(settings: StressSettings, density: f32) -> Result<(), Error> {
    let limits = [
        settings.compression_elastic_limit,
        settings.compression_fatal_limit,
        settings.tension_elastic_limit,
        settings.tension_fatal_limit,
        settings.shear_elastic_limit,
        settings.shear_fatal_limit,
    ];
    if settings.max_solver_iterations_per_frame == 0
        || !density.is_finite()
        || density <= 0.0
        || limits.iter().any(|value| !value.is_finite())
        || settings.compression_elastic_limit < 0.0
        || settings.compression_fatal_limit <= settings.compression_elastic_limit
    {
        Err(Error::InvalidStressSettings)
    } else {
        Ok(())
    }
}
