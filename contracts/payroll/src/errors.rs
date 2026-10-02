use soroban_sdk::contracterror;

/// Errors returned by the payroll contract. The `u32` codes are stable and
/// surface in transaction results, so never renumber them.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum Error {
    /// The contract has not been initialized (no admin/token set).
    NotInitialized = 1,
    /// Amount must be positive (deposit/pay) or non-negative (salary).
    InvalidAmount = 2,
    /// The worker has no salary configured.
    NoSalarySet = 3,
    /// The treasury does not hold enough tokens to cover the payout.
    InsufficientTreasury = 4,
    /// A batch carries more payments than one transaction may hold.
    BatchTooLarge = 5,
    /// A batch must contain at least one payment.
    EmptyBatch = 6,
    /// This run id was already paid; a retry must not pay twice.
    RunAlreadyPaid = 7,
    /// `upgrade` needs a non-empty version label.
    InvalidVersion = 8,
    /// `migrate` already ran for the current version.
    AlreadyMigrated = 9,
}
