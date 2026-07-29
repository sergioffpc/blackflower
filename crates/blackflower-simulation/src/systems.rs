use blackflower_ecs::{Error, PhaseId, Tag, World};
use strum::IntoStaticStr;

use crate::{SimulationPhase, SimulationPipeline, telemetry};

#[derive(Tag)]
struct SimulationSystemDriver;

/// A system in the authoritative [`SimulationPhase::PrepareTick`] phase.
///
/// These systems currently provide only observability. Their names and phase
/// assignments establish the intended responsibility boundaries without
/// mutating authoritative state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, IntoStaticStr)]
pub enum PrepareTickSystem {
    /// Open the next tick and initialize its tick-local working context.
    OpenTick,
    /// Activate accepted commits whose scheduled activation tick is now.
    ActivateScheduledCommits,
}

impl PrepareTickSystem {
    /// Number of systems currently registered in `PrepareTick`.
    pub const COUNT: usize = 2;

    /// Stable registration order for `PrepareTick` systems.
    pub const ORDER: [Self; Self::COUNT] = [Self::OpenTick, Self::ActivateScheduledCommits];

    /// Stable Flecs entity and trace field name for this system.
    #[must_use]
    pub fn name(self) -> &'static str {
        self.into()
    }
}

pub(crate) fn register(world: &mut World, pipeline: SimulationPipeline) -> Result<(), Error> {
    let driver = world.register_tag::<SimulationSystemDriver>()?;
    let driver_entity = world.spawn()?;
    world.add_tag(driver_entity, driver)?;

    register_prepare_tick_systems(
        world,
        pipeline.phase(SimulationPhase::PrepareTick),
        <SimulationSystemDriver as Tag>::NAME,
    )
}

fn register_prepare_tick_systems(
    world: &mut World,
    phase: PhaseId,
    driver_expression: &'static str,
) -> Result<(), Error> {
    for system in PrepareTickSystem::ORDER {
        world
            .system(system.name(), driver_expression)?
            .phase(phase)?
            .project(())?
            .each(move |_context, _entity, ()| {
                telemetry::system_executed(SimulationPhase::PrepareTick, system.name());
                Ok(())
            })?;
    }
    Ok(())
}
