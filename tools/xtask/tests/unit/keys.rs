use super::*;

#[test]
fn server_name_validation_accepts_dns_names() -> anyhow::Result<()> {
    validate_server_name("localhost")?;
    validate_server_name("game-1.internal.example")?;
    Ok(())
}

#[test]
fn server_name_validation_rejects_non_dns_input() {
    for server_name in [
        "",
        ".localhost",
        "localhost.",
        "-localhost",
        "localhost-",
        "local_host",
        "localhost\nsubjectAltName=DNS:attacker.example",
    ] {
        assert!(
            validate_server_name(server_name).is_err(),
            "{server_name:?}"
        );
    }
}

#[test]
fn relative_output_is_resolved_from_the_workspace() -> anyhow::Result<()> {
    let workspace = tempfile::tempdir()?;
    let absolute = workspace.path().join("absolute-keys");
    assert_eq!(
        resolve_output(workspace.path(), Path::new(".local-network")),
        workspace.path().join(".local-network")
    );
    assert_eq!(resolve_output(workspace.path(), &absolute), absolute);
    Ok(())
}

#[test]
fn generation_refuses_to_replace_an_existing_directory() -> anyhow::Result<()> {
    let workspace = tempfile::tempdir()?;
    let output = workspace.path().join("existing");
    fs::create_dir(&output)?;
    let args = GenerateArgs {
        output: PathBuf::from("existing"),
        server_name: String::from("localhost"),
    };

    let Err(error) = generate(workspace.path(), &args, Path::new("missing-openssl")) else {
        anyhow::bail!("an existing key directory was replaced");
    };
    assert!(error.to_string().contains("refusing to replace"));
    Ok(())
}

#[test]
fn generation_with_openssl_produces_the_complete_fixture() -> anyhow::Result<()> {
    let openssl = Path::new("openssl");
    if Command::new(openssl).arg("version").output().is_err() {
        eprintln!("skipping OpenSSL fixture test because `openssl` is unavailable");
        return Ok(());
    }

    let workspace = tempfile::tempdir()?;
    let args = GenerateArgs {
        output: PathBuf::from("credentials"),
        server_name: String::from("localhost"),
    };
    generate(workspace.path(), &args, openssl)?;

    let output = workspace.path().join("credentials");
    for file_name in [
        "service-ca-key.pem",
        "service-ca.pem",
        "server-key.pem",
        "server-leaf.pem",
        "server-chain.pem",
        "asset-signing-key.pem",
        "asset-signing-public.pem",
    ] {
        assert!(output.join(file_name).is_file(), "{file_name}");
    }
    for intermediate in ["service-ca.srl", "server.csr", "server.ext"] {
        assert!(!output.join(intermediate).exists(), "{intermediate}");
    }

    let private_pem = fs::read_to_string(output.join("asset-signing-key.pem"))?;
    let _signing_key = blackflower_assets::AssetSigningKey::from_pkcs8_pem(&private_pem)?;
    let public_pem = fs::read_to_string(output.join("asset-signing-public.pem"))?;
    let mut trust_store = blackflower_assets::AssetTrustStore::new();
    let _key_id = trust_store.trust_public_key_pem(&public_pem)?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        assert_eq!(fs::metadata(&output)?.permissions().mode() & 0o777, 0o700);
        for private_key in [
            "service-ca-key.pem",
            "server-key.pem",
            "asset-signing-key.pem",
        ] {
            assert_eq!(
                fs::metadata(output.join(private_key))?.permissions().mode() & 0o777,
                0o600,
                "{private_key}"
            );
        }
    }
    Ok(())
}
