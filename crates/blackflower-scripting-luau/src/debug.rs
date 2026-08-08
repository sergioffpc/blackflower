use glam::Vec3;

/// Reason the embedded Luau debugger invoked the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugEventKind {
    /// Execution reached a configured source breakpoint.
    Breakpoint,
    /// Execution completed one instruction while single-step mode was active.
    Step,
}

/// Execution mode selected by the host after inspecting a debug event.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum DebugAction {
    /// Resume normal execution.
    #[default]
    Continue,
    /// Execute one instruction and invoke the debugger again.
    Step,
}

/// Snapshot of a Luau value that is safe to copy out of a paused VM.
#[derive(Debug, Clone, PartialEq)]
pub enum DebugValue {
    /// Luau `nil`.
    Nil,
    /// Luau boolean.
    Boolean(bool),
    /// Luau double-precision number.
    Number(f64),
    /// Luau 64-bit integer.
    Integer(i64),
    /// Luau byte string.
    String(Box<[u8]>),
    /// Luau's default three-component vector.
    Vector(Vec3),
    /// A value intentionally kept inside the VM.
    Opaque {
        /// Stable Luau type name.
        type_name: String,
    },
}

/// Named local or upvalue captured from a paused stack frame.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugVariable {
    /// Source-level variable name.
    pub name: String,
    /// Safe copied representation of the current value.
    pub value: DebugValue,
}

/// One Luau call frame captured while execution is paused.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugFrame {
    /// Zero-based depth, where zero is the currently executing function.
    pub depth: u32,
    /// Chunk name supplied when the bytecode was loaded.
    pub source: Option<String>,
    /// Source-level function name when available.
    pub function: Option<String>,
    /// Current one-based source line when line information was compiled.
    pub current_line: Option<u32>,
    /// One-based line where the function was defined.
    pub defined_line: Option<u32>,
    /// Locals visible in this frame. Names require full debug information.
    pub locals: Vec<DebugVariable>,
    /// Upvalues captured by this frame. Names require full debug information.
    pub upvalues: Vec<DebugVariable>,
}

/// Complete synchronous snapshot delivered to a host debugger.
#[derive(Debug, Clone, PartialEq)]
pub struct DebugEvent {
    /// Why the debugger was invoked.
    pub kind: DebugEventKind,
    /// Call stack ordered from the active frame to its callers.
    pub frames: Vec<DebugFrame>,
}

/// Host-side debugger invoked synchronously on the runtime's owning thread.
///
/// A handler may block while an external debugger waits for a continue or step
/// command. It must not call back into the same [`crate::Runtime`].
pub trait DebugHandler {
    /// Inspect one event and choose how execution should resume.
    fn on_event(&mut self, event: &DebugEvent) -> DebugAction;
}

impl<F> DebugHandler for F
where
    F: FnMut(&DebugEvent) -> DebugAction,
{
    fn on_event(&mut self, event: &DebugEvent) -> DebugAction {
        self(event)
    }
}

/// Breakpoints and initial stepping state for one execution.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DebugOptions {
    breakpoints: Vec<u32>,
    single_step: bool,
}

impl DebugOptions {
    /// Add a requested one-based source breakpoint.
    #[must_use]
    pub fn with_breakpoint(mut self, line: u32) -> Self {
        self.breakpoints.push(line);
        self
    }

    /// Start execution in single-step mode.
    #[must_use]
    pub const fn with_single_step(mut self, enabled: bool) -> Self {
        self.single_step = enabled;
        self
    }

    /// Requested one-based source lines.
    #[must_use]
    pub fn breakpoints(&self) -> &[u32] {
        &self.breakpoints
    }

    /// Whether execution begins in single-step mode.
    #[must_use]
    pub const fn single_step(&self) -> bool {
        self.single_step
    }
}
