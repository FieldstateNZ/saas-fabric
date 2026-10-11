//! `fabric-api --contracts` reports what the binary was built with, with no
//! configuration, no environment and no network (#124).

use std::process::Command;

#[test]
fn prints_the_reader_contract_and_exits_zero_without_configuration() {
    let output = Command::new(env!("CARGO_BIN_EXE_fabric-api"))
        .arg("--contracts")
        .env_clear()
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout.clone()).unwrap(),
        format!(
            "{{\"binary\":\"fabric-api\",\"readerContract\":{},\"version\":\"{}\"}}\n",
            fabric_tenant_runtime::READER_CONTRACT,
            env!("CARGO_PKG_VERSION")
        )
    );
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn a_configuration_path_beside_the_flag_is_never_read() {
    let output = Command::new(env!("CARGO_BIN_EXE_fabric-api"))
        .args(["/nonexistent.toml", "--contracts"])
        .env_clear()
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
}
