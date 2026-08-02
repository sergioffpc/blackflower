use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use blackflower_rendering_fluids::{
    Backend, BackendError, BufferDesc, Context, Error, ResourceId, flow_version,
};

struct ProbeBackend {
    next_id: u64,
    buffers: BTreeMap<ResourceId, Vec<u8>>,
    created: Arc<AtomicUsize>,
    destroyed: Arc<AtomicUsize>,
}

impl Backend for ProbeBackend {
    fn current_frame(&self) -> u64 {
        7
    }

    fn last_completed_frame(&self) -> u64 {
        6
    }

    fn create_buffer(&mut self, desc: BufferDesc) -> Result<ResourceId, BackendError> {
        let length = usize::try_from(desc.size_in_bytes)
            .map_err(|_error| BackendError::Rejected("test buffer size"))?;
        self.next_id = self.next_id.saturating_add(1);
        let id = ResourceId::new(self.next_id).ok_or(BackendError::InvalidResourceId)?;
        self.buffers.insert(id, vec![0xFF; length]);
        self.created.fetch_add(1, Ordering::Relaxed);
        Ok(id)
    }

    fn destroy_buffer(&mut self, buffer: ResourceId) -> Result<(), BackendError> {
        self.buffers
            .remove(&buffer)
            .ok_or(BackendError::Rejected("destroy unknown test buffer"))?;
        self.destroyed.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }

    fn map_buffer(&mut self, buffer: ResourceId) -> Result<&mut [u8], BackendError> {
        self.buffers
            .get_mut(&buffer)
            .map(Vec::as_mut_slice)
            .ok_or(BackendError::Rejected("map unknown test buffer"))
    }

    fn unmap_buffer(&mut self, buffer: ResourceId) -> Result<(), BackendError> {
        if self.buffers.contains_key(&buffer) {
            Ok(())
        } else {
            Err(BackendError::Rejected("unmap unknown test buffer"))
        }
    }
}

#[test]
fn optimized_context_drives_backend_upload_lifecycle() -> Result<(), Error> {
    assert_eq!(flow_version(), "2.2.0");
    let created = Arc::new(AtomicUsize::new(0));
    let destroyed = Arc::new(AtomicUsize::new(0));
    {
        let backend = ProbeBackend {
            next_id: 0,
            buffers: BTreeMap::new(),
            created: Arc::clone(&created),
            destroyed: Arc::clone(&destroyed),
        };
        let mut context = Context::new(backend)?;
        context.set_min_resource_lifetime(2)?;
        context.validate_upload_path(256)?;
        assert_eq!(created.load(Ordering::Relaxed), 1);
        assert_eq!(context.backend().buffers.len(), 1);
        let bytes = context
            .backend()
            .buffers
            .values()
            .next()
            .ok_or(BackendError::Rejected("missing pooled upload buffer"))?;
        assert!(bytes.iter().all(|byte| *byte == 0));
    }
    assert_eq!(destroyed.load(Ordering::Relaxed), 1);
    Ok(())
}

#[test]
fn rejects_zero_length_validation_before_ffi() -> Result<(), Error> {
    let backend = ProbeBackend {
        next_id: 0,
        buffers: BTreeMap::new(),
        created: Arc::new(AtomicUsize::new(0)),
        destroyed: Arc::new(AtomicUsize::new(0)),
    };
    let mut context = Context::new(backend)?;
    assert_eq!(context.validate_upload_path(0), Err(Error::InvalidArgument));
    Ok(())
}
