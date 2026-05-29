use std::sync::Arc;

use anyhow::Context;
use blackflower_core::math::Vec3;
use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use tracing::{error, info, warn};

use crate::{
    camera::CameraResources,
    geometry::{CUBE_INDICES, CUBE_VERTICES, GeometryResources, Vertex},
};

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    device: wgpu::Device,
    queue: wgpu::Queue,

    pipeline: wgpu::RenderPipeline,
    depth_view: wgpu::TextureView,

    camera_resources: CameraResources,
    geometry_resources: GeometryResources,
}

impl Renderer {
    pub fn new_blocking<T>(target: Arc<T>, width: u32, height: u32) -> anyhow::Result<Self>
    where
        T: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static,
    {
        pollster::block_on(Self::new(target, width, height))
    }

    pub async fn new<T>(target: Arc<T>, width: u32, height: u32) -> anyhow::Result<Self>
    where
        T: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static,
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

        let aspect = width as f32 / height as f32;
        let camera_resources =
            CameraResources::new(&device, aspect, 60_f32.to_radians(), 0.1, 100.0);
        let geometry_resources = GeometryResources::new(&device, CUBE_VERTICES, CUBE_INDICES);

        let pipeline = Self::create_render_pipeline(
            &device,
            &config,
            &[Some(&camera_resources.bind_group_layout)],
        );
        let depth_view = Self::create_depth_view(&device, &config);

        Ok(Self {
            surface,
            config,
            device,
            queue,
            pipeline,
            depth_view,
            camera_resources,
            geometry_resources,
        })
    }

    pub fn render(&mut self) {
        self.camera_resources.camera.eye = Vec3::new(3.0, 2.0, 3.0);
        self.camera_resources.camera.target = Vec3::ZERO;
        self.camera_resources.camera.up = Vec3::Y;
        self.camera_resources.update_buffer(&self.queue);

        match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(surface_texture) => self.present(surface_texture),
            wgpu::CurrentSurfaceTexture::Suboptimal(surface_texture) => {
                warn!("suboptimal surface texture; still rendering");
                self.present(surface_texture);
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                warn!("skipping frame");
            }
            wgpu::CurrentSurfaceTexture::Outdated
            | wgpu::CurrentSurfaceTexture::Lost
            | wgpu::CurrentSurfaceTexture::Validation => {
                panic!("wgpu surface fatal error");
            }
        }
    }

    fn create_render_pipeline(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
        bind_group_layouts: &[Option<&wgpu::BindGroupLayout>],
    ) -> wgpu::RenderPipeline {
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Default render pipeline layout"),
            bind_group_layouts,
            immediate_size: 0,
        });
        device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Default render pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[Vertex::desc()],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: Some(wgpu::Face::Back),
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: wgpu::StencilState::default(),
                bias: wgpu::DepthBiasState::default(),
            }),
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        })
    }

    fn create_depth_view(
        device: &wgpu::Device,
        config: &wgpu::SurfaceConfiguration,
    ) -> wgpu::TextureView {
        let depth_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Depth texture"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        depth_texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    fn present(&self, surface_texture: wgpu::SurfaceTexture) {
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
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLUE),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
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

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.camera_resources.bind_group, &[]);
            pass.set_vertex_buffer(0, self.geometry_resources.vertex_buffer.slice(..));
            pass.set_index_buffer(
                self.geometry_resources.index_buffer.slice(..),
                wgpu::IndexFormat::Uint32,
            );
            pass.draw_indexed(0..self.geometry_resources.index_count, 0, 0..1);
        }

        self.queue.submit(std::iter::once(command_encoder.finish()));
        surface_texture.present();
    }
}
