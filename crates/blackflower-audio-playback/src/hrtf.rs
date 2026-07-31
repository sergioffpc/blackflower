use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use blackflower_audio_spatial::{AudioSettings, BinauralEffect, BinauralParams, Context, Vec3A};
use kira::Frame;
use kira::effect::{Effect, EffectBuilder};
use kira::info::Info;

#[derive(Clone)]
pub(crate) struct DirectionHandle {
    direction: Arc<[AtomicU32; 3]>,
}

impl DirectionHandle {
    pub(crate) fn set(&self, direction: [f32; 3]) {
        for (target, value) in self.direction.iter().zip(direction) {
            target.store(value.to_bits(), Ordering::Relaxed);
        }
    }
}

pub(crate) struct HrtfBuilder {
    direction: Arc<[AtomicU32; 3]>,
}

impl HrtfBuilder {
    pub(crate) fn new(direction: [f32; 3]) -> Self {
        Self {
            direction: Arc::new(direction.map(|value| AtomicU32::new(value.to_bits()))),
        }
    }
}

impl EffectBuilder for HrtfBuilder {
    type Handle = DirectionHandle;

    fn build(self) -> (Box<dyn Effect>, Self::Handle) {
        let handle = DirectionHandle {
            direction: Arc::clone(&self.direction),
        };
        (
            Box::new(HrtfEffect {
                direction: self.direction,
                frame_size: 0,
                context: None,
                effect: None,
                mono: Vec::new(),
                left: Vec::new(),
                right: Vec::new(),
            }),
            handle,
        )
    }
}

struct HrtfEffect {
    direction: Arc<[AtomicU32; 3]>,
    frame_size: usize,
    context: Option<Context>,
    effect: Option<BinauralEffect>,
    mono: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl HrtfEffect {
    fn rebuild(&mut self, sample_rate: u32) {
        self.context = None;
        self.effect = None;
        self.mono.resize(self.frame_size, 0.0);
        self.left.resize(self.frame_size, 0.0);
        self.right.resize(self.frame_size, 0.0);
        let Ok(frame_size) = u32::try_from(self.frame_size) else {
            return;
        };
        let Ok(settings) = AudioSettings::new(sample_rate, frame_size) else {
            return;
        };
        let Ok(mut context) = Context::new() else {
            return;
        };
        let Ok(hrtf) = context.create_default_hrtf(settings) else {
            return;
        };
        let Ok(effect) = context.create_binaural_effect(&hrtf) else {
            return;
        };
        self.context = Some(context);
        self.effect = Some(effect);
    }

    fn direction(&self) -> Option<BinauralParams> {
        let values = self
            .direction
            .each_ref()
            .map(|value| f32::from_bits(value.load(Ordering::Relaxed)));
        BinauralParams::new(Vec3A::from_array(values)).ok()
    }
}

impl Effect for HrtfEffect {
    fn init(&mut self, sample_rate: u32, internal_buffer_size: usize) {
        self.frame_size = internal_buffer_size;
        self.rebuild(sample_rate);
    }

    fn on_change_sample_rate(&mut self, sample_rate: u32) {
        self.rebuild(sample_rate);
    }

    fn process(&mut self, input: &mut [Frame], _dt: f64, _info: &Info) {
        let Some(params) = self.direction() else {
            input.fill(Frame::ZERO);
            return;
        };
        let Some(effect) = self.effect.as_mut() else {
            input.fill(Frame::ZERO);
            return;
        };
        self.mono.fill(0.0);
        self.left.fill(0.0);
        self.right.fill(0.0);
        for (target, frame) in self.mono.iter_mut().zip(input.iter()) {
            *target = (frame.left + frame.right) * 0.5;
        }
        if effect
            .process_mono(params, &self.mono, &mut self.left, &mut self.right)
            .is_err()
        {
            input.fill(Frame::ZERO);
            return;
        }
        for ((frame, left), right) in input.iter_mut().zip(&self.left).zip(&self.right) {
            *frame = Frame::new(*left, *right);
        }
    }
}
