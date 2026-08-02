//! Contract and lifecycle support for the isolated interchain test harness.

pub mod contract;
pub mod manifest;

#[cfg(test)]
mod tests {
    use std::path::Path;

    #[test]
    fn crate_belongs_to_the_nested_workspace() {
        let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
        let workspace_dir = crate_dir
            .parent()
            .and_then(Path::parent)
            .expect("harness crate should be under the nested workspace");
        let repository_dir = workspace_dir
            .parent()
            .and_then(Path::parent)
            .expect("nested workspace should be under tools");

        assert!(workspace_dir.join("rust-toolchain.toml").is_file());
        assert!(workspace_dir.join("Cargo.lock").is_file());
        assert_ne!(
            workspace_dir.join("Cargo.toml"),
            repository_dir.join("Cargo.toml")
        );
    }
}
