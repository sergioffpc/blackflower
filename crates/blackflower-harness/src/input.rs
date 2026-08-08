use blackflower_networking::{
    CommandId, ControlFrame, DatagramHeader, DiscreteCommand, FlowId, FlowSequence, InputDatagram,
    InputSequence, MAX_COMMAND_BYTES, MAX_COMMANDS, MAX_CONTROL_FRAME_BYTES, MAX_CONTROL_FRAMES,
    SnapshotAppliedAck, WireError, encode_datagram, encode_input_datagram,
};
use bytes::Bytes;
use std::collections::{BTreeMap, VecDeque};

use crate::{CommandSubmission, ControlBinding, ControlSubmission};

#[derive(Clone)]
pub(crate) struct InputSender {
    connection_epoch: blackflower_networking::ConnectionEpoch,
    binding: Option<ControlBinding>,
    next_input_sequence: u64,
    next_command_id: u64,
    next_flow_sequence: u32,
    recent_frames: VecDeque<ControlFrame>,
    pending_commands: BTreeMap<CommandId, DiscreteCommand>,
    applied_snapshot: Option<SnapshotAppliedAck>,
}

impl InputSender {
    pub(crate) fn new(connection_epoch: blackflower_networking::ConnectionEpoch) -> Self {
        Self {
            connection_epoch,
            binding: None,
            next_input_sequence: 1,
            next_command_id: 1,
            next_flow_sequence: 0,
            recent_frames: VecDeque::with_capacity(MAX_CONTROL_FRAMES),
            pending_commands: BTreeMap::new(),
            applied_snapshot: None,
        }
    }

    pub(crate) fn set_binding(&mut self, binding: ControlBinding) {
        if self.binding.is_some_and(|current| current != binding) {
            self.recent_frames.clear();
            self.pending_commands.clear();
        }
        self.binding = Some(binding);
    }

    pub(crate) fn reset_control_timeline(&mut self) {
        self.recent_frames.clear();
    }

    pub(crate) fn set_applied_snapshot(&mut self, ack: SnapshotAppliedAck) {
        self.applied_snapshot = Some(ack);
    }

    pub(crate) fn acknowledge_command(&mut self, command_id: CommandId) {
        drop(self.pending_commands.remove(&command_id));
    }

    pub(crate) fn reconnect(&mut self, epoch: blackflower_networking::ConnectionEpoch) {
        self.connection_epoch = epoch;
        self.next_flow_sequence = 0;
    }

    pub(crate) fn build(
        &mut self,
        submission: ControlSubmission,
    ) -> Result<(InputSequence, ControlFrame, Bytes), InputBuildError> {
        let binding = self.binding.ok_or(InputBuildError::MissingBinding)?;
        self.validate_submission(&submission)?;
        let sequence = self.allocate_input_sequence()?;
        let frame = ControlFrame {
            sequence,
            execute_tick: submission.execute_tick,
            payload: submission.payload,
        };
        let commands = self.build_commands(sequence, submission.commands)?;
        self.record_frame(frame.clone());
        self.record_commands(commands);
        let datagram = self.encode_current(binding)?;
        Ok((sequence, frame, datagram))
    }

    fn validate_submission(&self, submission: &ControlSubmission) -> Result<(), InputBuildError> {
        if submission.payload.len() > MAX_CONTROL_FRAME_BYTES {
            return Err(InputBuildError::ControlPayloadTooLarge);
        }
        if submission
            .commands
            .iter()
            .any(|command| command.payload.len() > MAX_COMMAND_BYTES)
        {
            return Err(InputBuildError::CommandPayloadTooLarge);
        }
        if submission
            .commands
            .len()
            .saturating_add(self.pending_commands.len())
            > MAX_COMMANDS
        {
            return Err(InputBuildError::TooManyPendingCommands);
        }
        let Some(current) = self.recent_frames.front() else {
            return Ok(());
        };
        let expected = current
            .execute_tick
            .get()
            .checked_add(4)
            .ok_or(InputBuildError::SequenceExhausted)?;
        if submission.execute_tick.get() != expected {
            return Err(InputBuildError::NonConsecutiveControlTick {
                expected: blackflower_networking::SimulationTick::new(expected),
                actual: submission.execute_tick,
            });
        }
        Ok(())
    }

    fn build_commands(
        &mut self,
        sequence: InputSequence,
        submissions: Vec<CommandSubmission>,
    ) -> Result<Vec<DiscreteCommand>, InputBuildError> {
        submissions
            .into_iter()
            .map(|submission| {
                Ok(DiscreteCommand {
                    command_id: self.allocate_command_id()?,
                    origin_input_sequence: sequence,
                    execute_tick: submission.execute_tick,
                    view_tick: submission.view_tick,
                    timing_class: submission.timing_class,
                    kind: submission.kind,
                    payload: submission.payload,
                })
            })
            .collect()
    }

    fn record_frame(&mut self, frame: ControlFrame) {
        self.recent_frames.push_front(frame);
        while self.recent_frames.len() > MAX_CONTROL_FRAMES {
            drop(self.recent_frames.pop_back());
        }
    }

    fn record_commands(&mut self, commands: Vec<DiscreteCommand>) {
        for command in commands {
            self.pending_commands.insert(command.command_id, command);
        }
    }

    fn encode_current(&mut self, binding: ControlBinding) -> Result<Bytes, InputBuildError> {
        let input = InputDatagram {
            control_epoch: binding.control_epoch,
            controlled_entity: binding.controlled_entity,
            frames: self.recent_frames.iter().cloned().collect(),
            commands: self.pending_commands.values().cloned().collect(),
            applied_snapshot: self.applied_snapshot,
        };
        let payload = encode_input_datagram(&input)?;
        let header = DatagramHeader {
            flow: FlowId::Input,
            connection_epoch: self.connection_epoch,
            flow_sequence: self.allocate_flow_sequence()?,
        };
        Ok(encode_datagram(header, &payload))
    }

    fn allocate_input_sequence(&mut self) -> Result<InputSequence, InputBuildError> {
        let current = self.next_input_sequence;
        self.next_input_sequence = current
            .checked_add(1)
            .ok_or(InputBuildError::SequenceExhausted)?;
        Ok(InputSequence::new(current))
    }

    fn allocate_command_id(&mut self) -> Result<CommandId, InputBuildError> {
        let current = self.next_command_id;
        self.next_command_id = current
            .checked_add(1)
            .ok_or(InputBuildError::SequenceExhausted)?;
        Ok(CommandId::new(current))
    }

    fn allocate_flow_sequence(&mut self) -> Result<FlowSequence, InputBuildError> {
        let current = self.next_flow_sequence;
        self.next_flow_sequence = current
            .checked_add(1)
            .ok_or(InputBuildError::SequenceExhausted)?;
        Ok(FlowSequence::new(current))
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InputBuildError {
    #[error("the server has not assigned a controlled entity")]
    MissingBinding,
    #[error("input, command, or flow sequence exhausted")]
    SequenceExhausted,
    #[error("more than eight commands are awaiting server acknowledgement")]
    TooManyPendingCommands,
    #[error("canonical control payload exceeds 256 bytes")]
    ControlPayloadTooLarge,
    #[error("canonical command payload exceeds 128 bytes")]
    CommandPayloadTooLarge,
    #[error("control tick {actual} is not the required next tick {expected}")]
    NonConsecutiveControlTick {
        expected: blackflower_networking::SimulationTick,
        actual: blackflower_networking::SimulationTick,
    },
    #[error(transparent)]
    Wire(#[from] WireError),
}
