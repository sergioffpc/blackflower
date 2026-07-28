use std::ffi::CString;

use crate::error::Error;
use crate::ffi;
use crate::ids::PipelineId;
use crate::telemetry::{self, ResourceKind};
use crate::world::World;

/// Builder for a Flecs pipeline query.
pub struct PipelineBuilder<'world> {
    world: &'world mut World,
    name: String,
    name_c: CString,
    expression: String,
    expression_c: CString,
}

impl<'world> PipelineBuilder<'world> {
    pub(crate) fn new(
        world: &'world mut World,
        name: &str,
        expression: &str,
    ) -> Result<Self, Error> {
        if name.is_empty() {
            return Err(Error::InvalidName);
        }
        let name_c = CString::new(name).map_err(|_error| Error::InvalidName)?;
        let expression_c = CString::new(expression).map_err(|_error| Error::InvalidExpression)?;
        Ok(Self {
            world,
            name: name.to_owned(),
            name_c,
            expression: expression.to_owned(),
            expression_c,
        })
    }

    /// Create the pipeline and return its world-bound handle.
    pub fn build(self) -> Result<PipelineId, Error> {
        let entity = self.world.create_named_system_entity(&self.name_c)?;
        let Some(raw) =
            ffi::create_pipeline(self.world.pointer, entity, self.expression_c.as_c_str())
        else {
            self.world.delete_raw_entity(entity);
            return Err(Error::PipelineCreation(format!(
                "{}: {}",
                self.name, self.expression
            )));
        };
        telemetry::resource_registered(self.world.key, ResourceKind::Pipeline, &self.name, None);
        Ok(ffi::pipeline_id(self.world.key, raw))
    }
}
