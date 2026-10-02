use soroban_sdk::contracterror;

/// Errors returned by the factory. Codes are stable; never renumber.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The factory has not been initialized (no owner / wasm hash).
    NotInitialized = 1,
    /// `upgrade` needs a non-empty version label.
    InvalidVersion = 2,
    /// `migrate` already ran for the current version.
    AlreadyMigrated = 3,
}
