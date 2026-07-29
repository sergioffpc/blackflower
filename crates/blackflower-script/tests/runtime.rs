use std::error::Error as StdError;

use blackflower_script::{CompileOptions, Error, Runtime, Value, compile, luau_version};

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
