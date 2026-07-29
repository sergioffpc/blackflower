use crate::{Error, SamplingRatio};

/// Stable index of one state in an [`AnimationGraph`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AnimationStateId(usize);

impl AnimationStateId {
    /// Return the numeric state index.
    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

/// Timing metadata for one host-selected animation graph state.
#[derive(Debug, Clone, PartialEq)]
pub struct AnimationState {
    name: String,
    duration: f32,
    speed: f32,
    looping: bool,
}

impl AnimationState {
    /// Construct a looping state with unit playback speed.
    pub fn new(name: impl Into<String>, duration: f32) -> Result<Self, Error> {
        if !duration.is_finite() || duration <= 0.0 {
            return Err(Error::InvalidStateDuration);
        }
        Ok(Self {
            name: name.into(),
            duration,
            speed: 1.0,
            looping: true,
        })
    }

    /// Set a finite, non-negative playback speed.
    pub fn with_speed(mut self, speed: f32) -> Result<Self, Error> {
        if !speed.is_finite() || speed < 0.0 {
            return Err(Error::InvalidPlaybackSpeed);
        }
        self.speed = speed;
        Ok(self)
    }

    /// Configure whether normalized time wraps after the clip duration.
    #[must_use]
    pub const fn with_looping(mut self, looping: bool) -> Self {
        self.looping = looping;
        self
    }

    /// Return the diagnostic state name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the source clip duration in seconds.
    #[must_use]
    pub const fn duration(&self) -> f32 {
        self.duration
    }

    /// Return the playback speed multiplier.
    #[must_use]
    pub const fn speed(&self) -> f32 {
        self.speed
    }

    /// Return whether normalized time wraps.
    #[must_use]
    pub const fn is_looping(&self) -> bool {
        self.looping
    }
}

#[derive(Debug, Clone, Copy)]
struct Transition {
    from: AnimationStateId,
    to: AnimationStateId,
    duration: f32,
}

#[derive(Debug, Clone, Copy)]
struct ActiveTransition {
    source: AnimationStateId,
    source_time: f32,
    target: AnimationStateId,
    target_time: f32,
    elapsed: f32,
    duration: f32,
}

/// One state sample and weight produced by graph evaluation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphLayer {
    state: AnimationStateId,
    ratio: SamplingRatio,
    weight: f32,
}

impl GraphLayer {
    /// Return the sampled state.
    #[must_use]
    pub const fn state(self) -> AnimationStateId {
        self.state
    }

    /// Return the normalized clip sample time.
    #[must_use]
    pub const fn ratio(self) -> SamplingRatio {
        self.ratio
    }

    /// Return the normalized graph contribution.
    #[must_use]
    pub const fn weight(self) -> f32 {
        self.weight
    }
}

/// Current graph output, containing one state or a two-state crossfade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GraphEvaluation {
    primary: GraphLayer,
    secondary: Option<GraphLayer>,
}

impl GraphEvaluation {
    /// Return the current or transition-source layer.
    #[must_use]
    pub const fn primary(self) -> GraphLayer {
        self.primary
    }

    /// Return the transition-target layer while crossfading.
    #[must_use]
    pub const fn secondary(self) -> Option<GraphLayer> {
        self.secondary
    }
}

/// Host-driven animation state graph with explicit directed transitions.
///
/// Gameplay or policy code decides when to request a registered transition.
/// The graph owns only timing and crossfade evaluation.
#[derive(Debug, Clone)]
pub struct AnimationGraph {
    states: Vec<AnimationState>,
    transitions: Vec<Transition>,
    current: AnimationStateId,
    state_time: f32,
    active: Option<ActiveTransition>,
}

impl AnimationGraph {
    /// Construct a graph with its initial state.
    #[must_use]
    pub fn new(initial: AnimationState) -> Self {
        Self {
            states: vec![initial],
            transitions: Vec::new(),
            current: AnimationStateId(0),
            state_time: 0.0,
            active: None,
        }
    }

    /// Add a state and return its stable graph-local identifier.
    pub fn add_state(&mut self, state: AnimationState) -> AnimationStateId {
        let id = AnimationStateId(self.states.len());
        self.states.push(state);
        id
    }

    /// Register a directed transition and its crossfade duration.
    pub fn add_transition(
        &mut self,
        from: AnimationStateId,
        to: AnimationStateId,
        duration: f32,
    ) -> Result<(), Error> {
        self.require_state(from)?;
        self.require_state(to)?;
        if !duration.is_finite() || duration < 0.0 {
            return Err(Error::InvalidTransitionDuration);
        }
        if self
            .transitions
            .iter()
            .any(|transition| transition.from == from && transition.to == to)
        {
            return Err(Error::DuplicateTransition);
        }
        self.transitions.push(Transition { from, to, duration });
        Ok(())
    }

    /// Begin a registered transition from the current state.
    pub fn transition_to(&mut self, target: AnimationStateId) -> Result<(), Error> {
        self.require_state(target)?;
        if self.active.is_some() {
            return Err(Error::TransitionInProgress);
        }
        let transition = self
            .transitions
            .iter()
            .find(|transition| transition.from == self.current && transition.to == target)
            .copied()
            .ok_or(Error::MissingTransition)?;
        if transition.duration == 0.0 {
            self.current = target;
            self.state_time = 0.0;
        } else {
            self.active = Some(ActiveTransition {
                source: self.current,
                source_time: self.state_time,
                target,
                target_time: 0.0,
                elapsed: 0.0,
                duration: transition.duration,
            });
        }
        Ok(())
    }

    /// Advance graph timing and return the current blend layers.
    pub fn advance(&mut self, delta_seconds: f32) -> Result<GraphEvaluation, Error> {
        if !delta_seconds.is_finite() || delta_seconds < 0.0 {
            return Err(Error::InvalidGraphDelta);
        }
        if let Some(mut active) = self.active {
            advance_transition(&self.states, &mut active, delta_seconds)?;
            if active.elapsed >= active.duration {
                self.current = active.target;
                self.state_time = active.target_time;
                self.active = None;
            } else {
                self.active = Some(active);
            }
        } else {
            self.state_time =
                advance_time(&self.states[self.current.0], self.state_time, delta_seconds)?;
        }
        Ok(self.evaluate())
    }

    /// Return the current graph evaluation without advancing time.
    #[must_use]
    pub fn evaluate(&self) -> GraphEvaluation {
        match self.active {
            Some(active) => transition_evaluation(&self.states, active),
            None => GraphEvaluation {
                primary: graph_layer(&self.states, self.current, self.state_time, 1.0),
                secondary: None,
            },
        }
    }

    /// Return the settled state, or the transition source while crossfading.
    #[must_use]
    pub const fn current_state(&self) -> AnimationStateId {
        self.current
    }

    /// Look up registered state metadata.
    #[must_use]
    pub fn state(&self, id: AnimationStateId) -> Option<&AnimationState> {
        self.states.get(id.0)
    }

    fn require_state(&self, id: AnimationStateId) -> Result<(), Error> {
        if self.states.get(id.0).is_some() {
            Ok(())
        } else {
            Err(Error::UnknownAnimationState(id.0))
        }
    }
}

fn advance_transition(
    states: &[AnimationState],
    active: &mut ActiveTransition,
    delta_seconds: f32,
) -> Result<(), Error> {
    active.source_time = advance_time(&states[active.source.0], active.source_time, delta_seconds)?;
    active.target_time = advance_time(&states[active.target.0], active.target_time, delta_seconds)?;
    active.elapsed += delta_seconds;
    if active.elapsed.is_finite() {
        Ok(())
    } else {
        Err(Error::InvalidGraphDelta)
    }
}

fn advance_time(state: &AnimationState, time: f32, delta_seconds: f32) -> Result<f32, Error> {
    let next = time + delta_seconds * state.speed;
    if next.is_finite() {
        Ok(next)
    } else {
        Err(Error::InvalidGraphDelta)
    }
}

fn transition_evaluation(states: &[AnimationState], active: ActiveTransition) -> GraphEvaluation {
    let target_weight = (active.elapsed / active.duration).clamp(0.0, 1.0);
    GraphEvaluation {
        primary: graph_layer(
            states,
            active.source,
            active.source_time,
            1.0 - target_weight,
        ),
        secondary: Some(graph_layer(
            states,
            active.target,
            active.target_time,
            target_weight,
        )),
    }
}

fn graph_layer(
    states: &[AnimationState],
    id: AnimationStateId,
    time: f32,
    weight: f32,
) -> GraphLayer {
    let state = &states[id.0];
    let normalized = time / state.duration;
    let ratio = if state.looping {
        normalized.rem_euclid(1.0)
    } else {
        normalized.clamp(0.0, 1.0)
    };
    GraphLayer {
        state: id,
        ratio: SamplingRatio::from_validated(ratio),
        weight,
    }
}
