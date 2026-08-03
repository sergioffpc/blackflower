use std::error::Error as StdError;

use blackflower_scripting_luau::{
    CompileOptions, DebugAction, DebugEventKind, DebugLevel, DebugOptions, DebugValue, Error,
    Library, MIN_NATIVE_CODEGEN_LIMIT_BYTES, OptimizationLevel, Runtime, RuntimeConfig,
    SandboxPolicy, TypeInfoLevel, Value, compile, luau_version, native_codegen_supported,
};

type TestResult = Result<(), Box<dyn StdError>>;

#[test]
fn reports_the_pinned_luau_version() {
    assert_eq!(luau_version(), (0, 731, 0));
}

#[test]
fn compiles_and_executes_primitive_results() -> TestResult {
    let bytecode = compile(
        "return true, 42.5, \"blackflower\", vector.create(1, 2, 3)",
        CompileOptions::default(),
    )?;
    assert!(!bytecode.is_empty());

    let mut runtime = Runtime::new()?;
    let values = runtime.execute_bytecode("primitive-results.luau", &bytecode)?;
    assert_eq!(values.len(), 4);
    assert_eq!(values.first(), Some(&Value::Boolean(true)));
    assert!(matches!(
        values.get(1),
        Some(Value::Number(number)) if number.to_bits() == 42.5_f64.to_bits()
    ));
    assert_eq!(values.get(2), Some(&Value::from("blackflower")));
    assert_eq!(values.get(3), Some(&Value::Vector([1.0, 2.0, 3.0])));
    Ok(())
}

#[test]
fn compilation_reports_encoded_syntax_errors_before_runtime_loading() {
    let error = compile("local =", CompileOptions::default());
    assert!(matches!(error, Err(Error::Compile(_))));
}

#[test]
fn compiler_options_change_bytecode_without_changing_results() -> TestResult {
    let source =
        "local function add(a: number, b: number): number return a + b end\nreturn add(20, 22)";
    let none = compile(
        source,
        CompileOptions {
            optimization: OptimizationLevel::None,
            ..CompileOptions::default()
        },
    )?;
    let baseline = compile(
        source,
        CompileOptions {
            optimization: OptimizationLevel::Baseline,
            ..CompileOptions::default()
        },
    )?;
    let aggressive = compile(
        source,
        CompileOptions {
            optimization: OptimizationLevel::Aggressive,
            ..CompileOptions::default()
        },
    )?;
    assert_ne!(none.as_bytes(), baseline.as_bytes());
    assert_ne!(baseline.as_bytes(), aggressive.as_bytes());

    let mut runtime = Runtime::new()?;
    for bytecode in [&none, &baseline, &aggressive] {
        assert_eq!(
            runtime.execute_bytecode("optimization.luau", bytecode)?,
            vec![Value::Number(42.0)]
        );
    }
    Ok(())
}

#[test]
fn preserves_runtime_globals_between_chunks() -> TestResult {
    let mut runtime = Runtime::new()?;
    assert!(
        runtime
            .execute(
                "define-policy.luau",
                "function decide() return \"advance\" end"
            )?
            .is_empty()
    );
    assert_eq!(
        runtime.execute("call-policy.luau", "return decide()")?,
        vec![Value::from("advance")]
    );
    Ok(())
}

#[test]
fn initializes_random_with_an_explicit_seed() -> TestResult {
    let mut first = Runtime::with_seed(91)?;
    let mut second = Runtime::with_seed(91)?;
    let source = "return math.random(), math.random(1, 1000)";
    assert_eq!(
        first.execute("first-random.luau", source)?,
        second.execute("second-random.luau", source)?
    );
    Ok(())
}

#[test]
fn excludes_nondeterministic_and_debug_libraries() -> TestResult {
    let mut runtime = Runtime::new()?;
    assert_eq!(
        runtime.execute(
            "sandbox.luau",
            "return os == nil, debug == nil, require == nil"
        )?,
        vec![
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true)
        ]
    );
    Ok(())
}

#[test]
fn configures_the_safe_library_allowlist() -> TestResult {
    let policy = SandboxPolicy::empty()
        .with_library(Library::Base, true)
        .with_library(Library::Math, true);
    let config = RuntimeConfig::default()
        .with_random_seed(31)
        .with_sandbox_policy(policy);
    let mut runtime = Runtime::with_config(config)?;

    assert_eq!(
        runtime.execute(
            "library-policy.luau",
            "return math ~= nil, table == nil, string == nil, vector == nil"
        )?,
        vec![
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true),
            Value::Boolean(true)
        ]
    );
    assert!(runtime.config().sandbox_policy().allows(Library::Math));
    assert!(!runtime.config().sandbox_policy().allows(Library::Table));
    Ok(())
}

#[test]
fn rejects_zero_resource_limits() {
    assert!(matches!(
        Runtime::with_config(RuntimeConfig::default().with_vm_memory_limit_bytes(0)),
        Err(Error::InvalidMemoryLimit)
    ));
    assert!(matches!(
        Runtime::with_config(RuntimeConfig::default().with_execution_fuel(0)),
        Err(Error::InvalidExecutionFuel)
    ));
    assert!(matches!(
        Runtime::with_config(
            RuntimeConfig::default()
                .with_native_codegen_limit_bytes(MIN_NATIVE_CODEGEN_LIMIT_BYTES - 1)
        ),
        Err(Error::InvalidNativeCodegenLimit)
    ));
    assert!(matches!(
        Runtime::with_config(RuntimeConfig::default().with_vm_memory_limit_bytes(1)),
        Err(Error::OutOfMemory)
    ));
}

#[test]
fn interrupts_execution_when_fuel_is_exhausted() -> TestResult {
    let config = RuntimeConfig::default().with_execution_fuel(16);
    let mut runtime = Runtime::with_config(config)?;

    assert_eq!(
        runtime.execute("fuel-limit.luau", "while true do end"),
        Err(Error::ExecutionLimit)
    );
    assert_eq!(
        runtime.execute(
            "caught-fuel-limit.luau",
            "return pcall(function() while true do end end)"
        ),
        Err(Error::ExecutionLimit)
    );
    assert_eq!(
        runtime.execute("after-fuel-limit.luau", "return true")?,
        vec![Value::Boolean(true)]
    );
    Ok(())
}

#[test]
fn excludes_non_preemptible_string_builtins() -> TestResult {
    let mut runtime = Runtime::new()?;

    assert_eq!(
        runtime.execute(
            "string-policy.luau",
            "return string.find == nil, string.match == nil, string.gmatch == nil, \
             string.gsub == nil, string.format == nil, string.pack == nil, \
             string.packsize == nil, string.unpack == nil"
        )?,
        vec![Value::Boolean(true); 8]
    );
    Ok(())
}

#[test]
fn bounds_native_string_operations_before_expansion() -> TestResult {
    let mut runtime = Runtime::new()?;

    let repetition = runtime.execute(
        "bounded-string-repetition.luau",
        "return string.rep(\"abcd\", 16_385)",
    );
    assert!(matches!(
        repetition,
        Err(Error::Runtime(message)) if message.contains("string result exceeds sandbox limit")
    ));

    let oversized_input = "x".repeat(65_537);
    let transformation = runtime.execute(
        "bounded-string-transformation.luau",
        &format!("return string.upper(\"{oversized_input}\")"),
    );
    assert!(matches!(
        transformation,
        Err(Error::Runtime(message)) if message.contains("string argument exceeds sandbox limit")
    ));
    assert_eq!(
        runtime.execute("after-string-limit.luau", "return string.upper(\"ok\")")?,
        vec![Value::from("OK")]
    );
    Ok(())
}

#[test]
fn rejects_vm_allocations_above_the_memory_limit() -> TestResult {
    const MEMORY_LIMIT_BYTES: usize = 1024 * 1024;

    let config = RuntimeConfig::default().with_vm_memory_limit_bytes(MEMORY_LIMIT_BYTES);
    let mut runtime = Runtime::with_config(config)?;
    let initial_usage = runtime.memory_usage();
    assert_eq!(initial_usage.limit_bytes, MEMORY_LIMIT_BYTES);
    assert!(initial_usage.current_bytes < initial_usage.limit_bytes);

    assert_eq!(
        runtime.execute(
            "memory-limit.luau",
            "local value = buffer.create(8 * 1024 * 1024) return buffer.len(value)"
        ),
        Err(Error::OutOfMemory)
    );
    let usage = runtime.memory_usage();
    assert!(usage.current_bytes <= usage.limit_bytes);
    assert!(usage.peak_bytes <= usage.limit_bytes);
    assert_eq!(
        runtime.execute("after-memory-limit.luau", "return true")?,
        vec![Value::Boolean(true)]
    );
    Ok(())
}

#[test]
fn reports_compile_and_runtime_errors() -> TestResult {
    let mut runtime = Runtime::new()?;
    let compile_error = runtime.execute("syntax-error.luau", "local =");
    assert!(matches!(compile_error, Err(Error::Compile(_))));

    let runtime_error = runtime.execute("runtime-error.luau", "error(\"failure\")");
    assert!(matches!(runtime_error, Err(Error::Runtime(_))));
    Ok(())
}

#[test]
fn line_info_adds_source_locations_and_stack_traces_to_runtime_errors() -> TestResult {
    let bytecode = compile(
        "local function fail()\n    error(\"failure\")\nend\nfail()",
        CompileOptions {
            debug: DebugLevel::LineInfo,
            ..CompileOptions::default()
        },
    )?;
    let mut runtime = Runtime::new()?;
    let Err(Error::Runtime(message)) = runtime.execute_bytecode("debug-trace.luau", &bytecode)
    else {
        return Err(std::io::Error::other("expected a Luau runtime error").into());
    };
    assert!(
        message.contains("debug-trace.luau:2"),
        "missing source location in `{message}`"
    );
    assert!(
        message.contains("stack trace:"),
        "missing stack trace in `{message}`"
    );
    assert!(
        message.contains("function fail"),
        "missing function name in `{message}`"
    );
    Ok(())
}

#[test]
fn full_debug_info_supports_breakpoints_steps_and_variable_inspection() -> TestResult {
    let bytecode = compile(
        "local captured = 5\nlocal function add(value)\n    local result = value + captured\n    return result\nend\nreturn add(37)",
        CompileOptions {
            optimization: OptimizationLevel::Baseline,
            debug: DebugLevel::Full,
            ..CompileOptions::default()
        },
    )?;
    let options = DebugOptions::default().with_breakpoint(3);
    let mut events = Vec::new();
    let mut handler = |event: &blackflower_scripting_luau::DebugEvent| {
        events.push(event.clone());
        if event.kind == DebugEventKind::Breakpoint {
            DebugAction::Step
        } else {
            DebugAction::Continue
        }
    };
    let mut runtime = Runtime::new()?;
    assert_eq!(
        runtime.execute_bytecode_debugged("debugger.luau", &bytecode, &options, &mut handler,)?,
        vec![Value::Number(42.0)]
    );

    let Some(breakpoint) = events
        .iter()
        .find(|event| event.kind == DebugEventKind::Breakpoint)
    else {
        return Err(std::io::Error::other("expected a breakpoint event").into());
    };
    let Some(active) = breakpoint.frames.first() else {
        return Err(std::io::Error::other("expected an active debug frame").into());
    };
    assert_eq!(active.current_line, Some(3));
    assert!(active.locals.iter().any(|variable| {
        variable.name == "value" && variable.value == DebugValue::Number(37.0)
    }));
    assert!(active.upvalues.iter().any(|variable| {
        variable.name == "captured" && variable.value == DebugValue::Number(5.0)
    }));
    assert!(
        events
            .iter()
            .any(|event| event.kind == DebugEventKind::Step)
    );
    Ok(())
}

#[test]
fn bytecode_without_line_info_rejects_source_breakpoints() -> TestResult {
    let bytecode = compile(
        "return 42",
        CompileOptions {
            debug: DebugLevel::None,
            ..CompileOptions::default()
        },
    )?;
    let options = DebugOptions::default().with_breakpoint(1);
    let mut handler = |_event: &blackflower_scripting_luau::DebugEvent| DebugAction::Continue;
    let mut runtime = Runtime::new()?;
    assert_eq!(
        runtime.execute_bytecode_debugged("no-debug-info.luau", &bytecode, &options, &mut handler,),
        Err(Error::InvalidBreakpoint { line: 1 })
    );
    Ok(())
}

#[test]
fn debugger_panics_are_contained_and_the_runtime_remains_usable() -> TestResult {
    let bytecode = compile(
        "local value = 41\nreturn value + 1",
        CompileOptions {
            debug: DebugLevel::Full,
            ..CompileOptions::default()
        },
    )?;
    let options = DebugOptions::default().with_breakpoint(2);
    let mut handler = |_event: &blackflower_scripting_luau::DebugEvent| -> DebugAction {
        std::panic::resume_unwind(Box::new("debugger failure"));
    };
    let mut runtime = Runtime::new()?;
    assert_eq!(
        runtime
            .execute_bytecode_debugged("debugger-panic.luau", &bytecode, &options, &mut handler,),
        Err(Error::DebugHandlerPanicked)
    );
    assert_eq!(
        runtime.execute("after-debugger-panic.luau", "return 42")?,
        vec![Value::Number(42.0)]
    );
    Ok(())
}

fn execute_codegen_source(
    runtime: &mut Runtime,
    chunk_name: &str,
    source: &str,
    type_info: TypeInfoLevel,
) -> Result<Vec<Value>, Error> {
    let bytecode = compile(
        source,
        CompileOptions {
            optimization: OptimizationLevel::Aggressive,
            type_info,
            ..CompileOptions::default()
        },
    )?;
    runtime.execute_bytecode(chunk_name, &bytecode)
}

#[test]
fn type_info_guides_bounded_native_codegen() -> TestResult {
    if !native_codegen_supported() {
        return Ok(());
    }

    let config = RuntimeConfig::default()
        .with_native_codegen_limit_bytes(2 * MIN_NATIVE_CODEGEN_LIMIT_BYTES);
    let mut runtime = Runtime::with_config(config)?;
    let source = "local function sum(limit)\n    local total = 0\n    for i = 1, limit do total += i end\n    return total\nend\nreturn sum(9)";

    assert_eq!(
        execute_codegen_source(
            &mut runtime,
            "interpreted.luau",
            source,
            TypeInfoLevel::NativeModules,
        )?,
        vec![Value::Number(45.0)]
    );
    assert_eq!(runtime.last_native_codegen_stats(), None);

    assert_eq!(
        execute_codegen_source(
            &mut runtime,
            "native-all.luau",
            source,
            TypeInfoLevel::AllModules,
        )?,
        vec![Value::Number(45.0)]
    );
    let Some(stats) = runtime.last_native_codegen_stats() else {
        return Err(std::io::Error::other("expected all-modules native compilation").into());
    };
    assert!(stats.functions_compiled > 0);
    assert!(stats.functions_bound > 0);
    assert!(stats.native_code_size_bytes > 0);

    assert_eq!(
        execute_codegen_source(
            &mut runtime,
            "native-module.luau",
            &format!("--!native\n{source}"),
            TypeInfoLevel::NativeModules,
        )?,
        vec![Value::Number(45.0)]
    );
    assert!(
        runtime
            .last_native_codegen_stats()
            .is_some_and(|stats| stats.functions_bound > 0)
    );

    let usage = runtime.native_codegen_memory_usage();
    assert!(usage.current_bytes > 0);
    assert!(usage.current_bytes <= usage.limit_bytes && usage.peak_bytes <= usage.limit_bytes);
    Ok(())
}

#[test]
fn debugger_temporarily_uses_the_interpreter_when_codegen_is_enabled() -> TestResult {
    if !native_codegen_supported() {
        return Ok(());
    }

    let bytecode = compile(
        "local function sum(limit)\n    local total = 0\n    for i = 1, limit do total += i end\n    return total\nend\nreturn sum(9)",
        CompileOptions {
            optimization: OptimizationLevel::Baseline,
            debug: DebugLevel::Full,
            type_info: TypeInfoLevel::AllModules,
            ..CompileOptions::default()
        },
    )?;
    let config = RuntimeConfig::default()
        .with_native_codegen_limit_bytes(2 * MIN_NATIVE_CODEGEN_LIMIT_BYTES);
    let mut runtime = Runtime::with_config(config)?;
    let options = DebugOptions::default().with_breakpoint(3);
    let mut breakpoint_hits = 0;
    let mut handler = |event: &blackflower_scripting_luau::DebugEvent| {
        if event.kind == DebugEventKind::Breakpoint {
            breakpoint_hits += 1;
        }
        DebugAction::Continue
    };
    assert_eq!(
        runtime.execute_bytecode_debugged(
            "native-debugger.luau",
            &bytecode,
            &options,
            &mut handler,
        )?,
        vec![Value::Number(45.0)]
    );
    assert_eq!(breakpoint_hits, 1);
    assert!(
        runtime
            .last_native_codegen_stats()
            .is_some_and(|stats| stats.functions_bound > 0)
    );

    assert_eq!(
        runtime.execute_bytecode("native-after-debugger.luau", &bytecode)?,
        vec![Value::Number(45.0)]
    );
    assert!(
        runtime
            .last_native_codegen_stats()
            .is_some_and(|stats| stats.functions_bound > 0)
    );
    Ok(())
}

#[test]
fn rejects_values_outside_the_safe_surface() -> TestResult {
    let mut runtime = Runtime::new()?;
    let result = runtime.execute("table-result.luau", "return {}");
    assert!(matches!(
        result,
        Err(Error::UnsupportedValue {
            index: 0,
            type_name
        }) if type_name == "table"
    ));
    Ok(())
}
