use derive_more::From;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, From)]
pub enum Error {
    InvalidPath(String),

    UnknownCommonRpcFnsEntry(String),
    EntityMissingFromRpcFns(String),
    SuffixMissingFromRpcFns,
    ForCreateMissingFromRpcFns,
    ForUpdateMissingFromRpcFns,
    FilterMissingFromRpcFns,
    CantMatchHandlerReturnType(String),
    CantMatchHandlerParams(String),

    #[from]
    Io(std::io::Error),
}

/* {{{ Region: boilerplate */

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:#?}")
    }
}

impl std::error::Error for Error {}

/* End Region:  boilerplate }}} */
