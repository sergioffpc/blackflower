use std::sync::Arc;

use anyhow::Context;
use blackflower_entity::EntityId;
use blackflower_math::{Vec3, components::Transform};
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use tracing::{debug, error, info, warn};

use crate::{
    camera::Camera,
    geometry::{CUBE_INDICES, CUBE_VERTICES, GeometryResources},
    pipelines::DefaultPipeline,
};

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,

    pipeline: DefaultPipeline,

    camera: Camera,
    geometry_resources: GeometryResources,
}

impl Renderer {
    pub fn new_blocking<T>(target: Arc<T>, width: u32, height: u32) -> anyhow::Result<Self>
    where
        T: HasDisplayHandle + HasWindowHandle + Send + Sync + ?Sized + 'static,
    {
        pollster::block_on(Self::new(target, width, height))
    }

    pub async fn new<T>(target: Arc<T>, width: u32, height: u32) -> anyhow::Result<Self>
    where
        T: HasDisplayHandle + HasWindowHandle + Send + Sync + ?Sized + 'static,
    {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            flags: wgpu::InstanceFlags::debugging(),
            memory_budget_thresholds: wgpu::MemoryBudgetThresholds::default(),
            backend_options: wgpu::BackendOptions::from_env_or_default(),
            display: None,
        });
        let surface = instance
            .create_surface(Arc::clone(&target))
            .context("creating renderer surface")?;
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            })
            .await
            .context("requesting GPU adapter")?;
        info!(adapter = ?adapter.get_info(), "adapter selected");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("Renderer device"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                experimental_features: wgpu::ExperimentalFeatures::default(),
                memory_hints: wgpu::MemoryHints::Performance,
                trace: wgpu::Trace::Off,
            })
            .await
            .context("requesting GPU device")?;
        device.set_device_lost_callback(|reason, message| {
            error!(reason = ?reason, message, "🚨 GPU device lost!");

            // IMPORTANT:
            // You should NOT try to use GPU resources anymore here.
            // Instead:
            // - signal your engine loop to stop rendering
            // - or schedule full GPU reinitialization
        });

        let surface_caps = surface.get_capabilities(&adapter);
        let surface_format = surface_caps
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .unwrap_or(surface_caps.formats[0]);
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width,
            height,
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };
        surface.configure(&device, &config);

        let pipeline = DefaultPipeline::new(&device, &config);

        let aspect = width as f32 / height as f32;
        let mut camera = Camera::new(aspect, 60_f32.to_radians(), 0.1, 100.0);
        camera.eye = Vec3::new(3.0, 2.0, 3.0);
        camera.target = Vec3::ZERO;
        camera.up = Vec3::Y;

        let geometry_resources = GeometryResources::new(&device, CUBE_VERTICES, CUBE_INDICES);

        Ok(Self {
            surface,
            config,
            device,
            queue,

            pipeline,

            camera,
            geometry_resources,
        })
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        self.config.width = width;
        self.config.height = height;
        self.surface.configure(&self.device, &self.config);
        self.pipeline = DefaultPipeline::new(&self.device, &self.config);
        self.camera.aspect = width as f32 / height as f32;
    }

    pub fn render(&mut self, instances: &[(EntityId, Transform)]) {
        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => {
                self.present(surface_texture, instances);
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                debug!("suboptimal surface texture; still rendering");
                self.present(surface_texture, instances);
                self.surface.configure(&self.device, &self.config);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {}
            wgpu::CurrentSurfaceTexture::Outdated => {
                debug!("surface outdated; reconfiguring");
                self.surface.configure(&self.device, &self.config);
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                warn!("surface lost; reconfiguring");
                self.surface.configure(&self.device, &self.config);
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                error!("wgpu validation error; skipping frame");
            }
        }
    }

    fn present(
        &mut self,
        surface_texture: wgpu::SurfaceTexture,
        instances: &[(EntityId, Transform)],
    ) {
        let surface_view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut command_encoder =
            self.device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("Present command encoder"),
                });
        {
            let mut pass = command_encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &surface_view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.04,
                            g: 0.04,
                            b: 0.04,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.pipeline.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });

            pass.set_pipeline(&self.pipeline.pipeline);
            pass.set_bind_group(0, &self.pipeline.camera_bind_group, &[]);
            self.pipeline
                .update_camera_uniform(&self.queue, &self.camera);

            for (_entity_id, transform) in instances {
                pass.set_bind_group(1, &self.pipeline.model_bind_group, &[]);
                self.pipeline.update_model_uniform(&self.queue, transform);
                pass.set_vertex_buffer(0, self.geometry_resources.vertex_buffer.slice(..));
                pass.set_index_buffer(
                    self.geometry_resources.index_buffer.slice(..),
                    wgpu::IndexFormat::Uint32,
                );
                pass.draw_indexed(0..self.geometry_resources.index_count, 0, 0..1);
            }
        }

        self.queue.submit(std::iter::once(command_encoder.finish()));
        surface_texture.present();
    }
}
