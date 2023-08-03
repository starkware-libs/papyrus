use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(test)]
#[path = "write_api_error_test.rs"]
mod write_api_error_test;

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum AddInvokeError {
    InsufficientAccountBalance(InsufficientAccountBalance),
    InsufficientMaxFee(InsufficientMaxFee),
    InvalidTransactionNonce(InvalidTransactionNonce),
    ValidationFailure(ValidationFailure),
    NonAccount(NonAccount),
    DuplicateTx(DuplicateTx),
    UnsupportedTxVersion(UnsupportedTxVersion),
    UnexpectedError(UnexpectedError),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum AddDeclareError {
    ClassAlreadyDeclared(ClassAlreadyDeclared),
    CompilationFailed(CompilationFailed),
    CompiledClassHashMismatch(CompiledClassHashMismatch),
    InsufficientAccountBalance(InsufficientAccountBalance),
    InsufficientMaxFee(InsufficientMaxFee),
    InvalidTransactionNonce(InvalidTransactionNonce),
    ValidationFailure(ValidationFailure),
    NonAccount(NonAccount),
    DuplicateTx(DuplicateTx),
    ContractClassSizeIsTooLarge(ContractClassSizeIsTooLarge),
    UnsupportedTxVersion(UnsupportedTxVersion),
    UnsupportedContractClassVersion(UnsupportedContractClassVersion),
    UnexpectedError(UnexpectedError),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub enum AddDeployAccountError {
    InsufficientAccountBalance(InsufficientAccountBalance),
    InsufficientMaxFee(InsufficientMaxFee),
    InvalidTransactionNonce(InvalidTransactionNonce),
    ValidationFailure(ValidationFailure),
    NonAccount(NonAccount),
    ClassHashNotFound(ClassHashNotFound),
    DuplicateTx(DuplicateTx),
    UnsupportedTxVersion(UnsupportedTxVersion),
    UnexpectedError(UnexpectedError),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum ClassHashNotFound {
    #[serde(rename = "Class hash not found")]
    Error(ErrorCode<28>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum ClassAlreadyDeclared {
    #[serde(rename = "Class already declared")]
    Error(ErrorCode<51>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum InvalidTransactionNonce {
    #[serde(rename = "Invalid transaction nonce")]
    Error(ErrorCode<52>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum InsufficientMaxFee {
    #[serde(rename = "\
        Max fee is smaller than the minimal transaction cost (validation plus fee transfer)")]
    Error(ErrorCode<53>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum InsufficientAccountBalance {
    #[serde(rename = "Account balance is smaller than the transaction's max_fee")]
    Error(ErrorCode<54>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum ValidationFailure {
    #[serde(rename = "Account validation failed")]
    Error(ErrorCode<55>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum CompilationFailed {
    #[serde(rename = "Compilation failed")]
    Error(ErrorCode<56>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum ContractClassSizeIsTooLarge {
    #[serde(rename = "Contract class size it too large")]
    Error(ErrorCode<57>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum NonAccount {
    #[serde(rename = "Sender address in not an account contract")]
    Error(ErrorCode<58>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum DuplicateTx {
    #[serde(rename = "A transaction with the same hash already exists in the mempool")]
    Error(ErrorCode<59>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum CompiledClassHashMismatch {
    #[serde(rename = "the compiled class hash did not match the one supplied in the transaction")]
    Error(ErrorCode<60>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum UnsupportedTxVersion {
    #[serde(rename = "the transaction version is not supported")]
    Error(ErrorCode<61>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum UnsupportedContractClassVersion {
    #[serde(rename = "the contract class version is not supported")]
    Error(ErrorCode<62>),
}

#[derive(Debug, Deserialize, Serialize, Clone, Eq, PartialEq)]
#[serde(tag = "message")]
pub enum UnexpectedError {
    #[serde(rename = "An unexpected error occured")]
    Error(ErrorCodeWithData<63>),
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ErrorCode<const CODE: usize> {
    code: ConstInt<CODE>,
}

#[derive(Debug, Default, Deserialize, Serialize, Clone, Eq, PartialEq)]
pub struct ErrorCodeWithData<const CODE: usize> {
    #[serde(flatten)]
    code: ErrorCode<CODE>,
    data: String,
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
pub struct ConstInt<const VALUE: usize>;

impl<const VALUE: usize> Serialize for ConstInt<VALUE> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(VALUE.try_into().expect("Failed converting a usize to u64."))
    }
}

impl<'de, const VALUE: usize> Deserialize<'de> for ConstInt<VALUE> {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = usize::deserialize(deserializer)?;
        if value == VALUE {
            Ok(Self)
        } else {
            Err(D::Error::custom(format!("Expected constant integer {VALUE}, got {value}.")))
        }
    }
}
