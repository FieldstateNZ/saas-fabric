//! `fabric-control-plane-api --contracts` reports what the publisher was built
//! with, with no configuration, no environment and no network (#124).

use std::process::Command;

#[test]
fn prints_the_publisher_contracts_and_exits_zero_without_configuration() {
    let output = Command::new(env!("CARGO_BIN_EXE_fabric-control-plane-api"))
        .arg("--contracts")
        .env_clear()
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
    assert_eq!(
        String::from_utf8(output.stdout.clone()).unwrap(),
        format!(
            "{{\"binary\":\"fabric-control-plane-api\",\"minimumReaderContract\":{},\
             \"minimumRollableVersion\":\"{}\",\"publisherContract\":{},\"version\":\"{}\"}}\n",
            fabric_runtime_publication::MINIMUM_READER_CONTRACT,
            fabric_component::MINIMUM_ROLLABLE_VERSION,
            fabric_runtime_publication::CONTRACT_VERSION,
            env!("CARGO_PKG_VERSION")
        )
    );
    assert!(output.stderr.is_empty(), "{output:?}");
}

#[test]
fn a_configuration_path_beside_the_flag_is_never_read() {
    let output = Command::new(env!("CARGO_BIN_EXE_fabric-control-plane-api"))
        .args(["/nonexistent.toml", "--contracts"])
        .env_clear()
        .output()
        .unwrap();

    assert!(output.status.success(), "{output:?}");
}
