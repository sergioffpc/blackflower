use std::error::Error as StdError;

use blackflower_script::{
    CompileOptions, Error, Library, Runtime, RuntimeConfig, SandboxPolicy, Value, compile,
    luau_version,
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
            "return string.rep(\"x\", 8 * 1024 * 1024)"
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
