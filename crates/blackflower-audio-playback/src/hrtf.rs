use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use blackflower_acoustics::PropagationDescriptor;
use blackflower_audio_media::AUDIO_SAMPLE_RATE;
use blackflower_audio_spatial::{
    AudioSettings, BinauralEffect, BinauralParams, Context, DirectEffect, PathEffect,
    PropagationExchange, Vec3A,
};
use kira::Frame;
use kira::effect::{Effect, EffectBuilder};
use kira::info::Info;

use crate::INTERNAL_BUFFER_SIZE;

#[derive(Clone)]
pub(crate) struct DirectionHandle {
    direction: Arc<[AtomicU32; 3]>,
    propagation: Option<Arc<PropagationExchange>>,
}

impl DirectionHandle {
    pub(crate) fn set(&self, direction: [f32; 3]) {
        for (target, value) in self.direction.iter().zip(direction) {
            target.store(value.to_bits(), Ordering::Relaxed);
        }
    }

    pub(crate) fn set_propagation(&self, propagation: PropagationDescriptor) {
        if let Some(exchange) = &self.propagation {
            exchange.publish(propagation);
            self.set(
                propagation
                    .direction_q15
                    .map(|value| f32::from(value) / f32::from(i16::MAX)),
            );
        }
    }
}

pub(crate) struct HrtfBuilder {
    direction: Arc<[AtomicU32; 3]>,
    propagation: Option<Arc<PropagationExchange>>,
    context: Option<Context>,
    effect: Option<BinauralEffect>,
    direct: Option<DirectEffect>,
    path: Option<PathEffect>,
    mono: Vec<f32>,
    scratch: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl HrtfBuilder {
    pub(crate) fn new(direction: [f32; 3], propagation: Option<PropagationDescriptor>) -> Self {
        let exchange = propagation.map(|value| Arc::new(PropagationExchange::new(value)));
        let (context, effect) = create_binaural_effect();
        Self {
            direction: Arc::new(direction.map(|value| AtomicU32::new(value.to_bits()))),
            direct: exchange
                .as_ref()
                .and_then(|_exchange| DirectEffect::new(INTERNAL_BUFFER_SIZE).ok()),
            path: exchange
                .as_ref()
                .and_then(|_exchange| PathEffect::new(INTERNAL_BUFFER_SIZE).ok()),
            propagation: exchange,
            context,
            effect,
            mono: vec![0.0; INTERNAL_BUFFER_SIZE],
            scratch: vec![0.0; INTERNAL_BUFFER_SIZE],
            left: vec![0.0; INTERNAL_BUFFER_SIZE],
            right: vec![0.0; INTERNAL_BUFFER_SIZE],
        }
    }
}

fn create_binaural_effect() -> (Option<Context>, Option<BinauralEffect>) {
    let Ok(frame_size) = u32::try_from(INTERNAL_BUFFER_SIZE) else {
        return (None, None);
    };
    let Ok(settings) = AudioSettings::new(AUDIO_SAMPLE_RATE, frame_size) else {
        return (None, None);
    };
    let Ok(mut context) = Context::new() else {
        return (None, None);
    };
    let Ok(hrtf) = context.create_default_hrtf(settings) else {
        return (Some(context), None);
    };
    let Ok(effect) = context.create_binaural_effect(&hrtf) else {
        return (Some(context), None);
    };
    (Some(context), Some(effect))
}

impl EffectBuilder for HrtfBuilder {
    type Handle = DirectionHandle;

    fn build(self) -> (Box<dyn Effect>, Self::Handle) {
        let handle = DirectionHandle {
            direction: Arc::clone(&self.direction),
            propagation: self.propagation.as_ref().map(Arc::clone),
        };
        (
            Box::new(HrtfEffect {
                direction: self.direction,
                propagation: self.propagation,
                enabled: false,
                _context: self.context,
                effect: self.effect,
                direct: self.direct,
                path: self.path,
                mono: self.mono,
                scratch: self.scratch,
                left: self.left,
                right: self.right,
            }),
            handle,
        )
    }
}

struct HrtfEffect {
    direction: Arc<[AtomicU32; 3]>,
    propagation: Option<Arc<PropagationExchange>>,
    enabled: bool,
    _context: Option<Context>,
    effect: Option<BinauralEffect>,
    direct: Option<DirectEffect>,
    path: Option<PathEffect>,
    mono: Vec<f32>,
    scratch: Vec<f32>,
    left: Vec<f32>,
    right: Vec<f32>,
}

impl HrtfEffect {
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
        self.enabled = sample_rate == AUDIO_SAMPLE_RATE
            && internal_buffer_size == INTERNAL_BUFFER_SIZE
            && self.effect.is_some();
    }

    fn on_change_sample_rate(&mut self, sample_rate: u32) {
        self.enabled = sample_rate == AUDIO_SAMPLE_RATE && self.effect.is_some();
    }

    fn process(&mut self, input: &mut [Frame], _dt: f64, _info: &Info) {
        if !self.enabled || input.len() != INTERNAL_BUFFER_SIZE {
            input.fill(Frame::ZERO);
            return;
        }
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
        if let (Some(exchange), Some(direct), Some(path)) = (
            self.propagation.as_ref(),
            self.direct.as_mut(),
            self.path.as_mut(),
        ) {
            let propagation = exchange.latest();
            if direct
                .process(propagation, &self.mono, &mut self.scratch)
                .and_then(|()| path.process(propagation, &self.scratch, &mut self.mono))
                .is_err()
            {
                input.fill(Frame::ZERO);
                return;
            }
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
